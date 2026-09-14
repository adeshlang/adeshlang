#![allow(dead_code)]

//! DNS Wire Format Encoder & Decoder with compression protection.

use super::message::{DNSHeader, DNSMessage, DNSQuestion};
use super::name::DNSName;
use super::records::{DNSClass, DNSRecord, DNSType, RData};
use crate::runtime::stdlib_src::net::ip::IPAddress;
use std::collections::HashMap;

const MAX_POINTER_DEPTH: usize = 16;

pub struct DNSWireEncoder {
    buf: Vec<u8>,
    name_offsets: HashMap<DNSName, u16>,
}

impl DNSWireEncoder {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(512),
            name_offsets: HashMap::new(),
        }
    }

    pub fn encode(msg: &DNSMessage) -> Result<Vec<u8>, String> {
        let mut encoder = Self::new();
        encoder.encode_header(&msg.header);

        for question in &msg.questions {
            encoder.encode_question(question)?;
        }

        for record in &msg.answers {
            encoder.encode_record(record)?;
        }

        for record in &msg.authorities {
            encoder.encode_record(record)?;
        }

        for record in &msg.additionals {
            encoder.encode_record(record)?;
        }

        Ok(encoder.buf)
    }

    fn encode_header(&mut self, header: &DNSHeader) {
        self.buf.extend_from_slice(&header.id.to_be_bytes());

        let mut flags: u16 = 0;
        if header.qr {
            flags |= 1 << 15;
        }
        flags |= ((header.opcode as u16) & 0x0F) << 11;
        if header.aa {
            flags |= 1 << 10;
        }
        if header.tc {
            flags |= 1 << 9;
        }
        if header.rd {
            flags |= 1 << 8;
        }
        if header.ra {
            flags |= 1 << 7;
        }
        if header.z {
            flags |= 1 << 6;
        }
        if header.ad {
            flags |= 1 << 5;
        }
        if header.cd {
            flags |= 1 << 4;
        }
        flags |= (header.rcode as u16) & 0x0F;

        self.buf.extend_from_slice(&flags.to_be_bytes());
        self.buf.extend_from_slice(&(header.qdcount).to_be_bytes());
        self.buf.extend_from_slice(&(header.ancount).to_be_bytes());
        self.buf.extend_from_slice(&(header.nscount).to_be_bytes());
        self.buf.extend_from_slice(&(header.arcount).to_be_bytes());
    }

    fn encode_name(&mut self, name: &DNSName) -> Result<(), String> {
        if let Some(&offset) = self.name_offsets.get(name) {
            let pointer: u16 = 0xC000 | offset;
            self.buf.extend_from_slice(&pointer.to_be_bytes());
            return Ok(());
        }

        if self.buf.len() < 0x3FFF {
            self.name_offsets
                .insert(name.clone(), self.buf.len() as u16);
        }

        for label in name.labels() {
            let bytes = label.as_bytes();
            if bytes.len() > 63 {
                return Err("Label too long for wire format".to_string());
            }
            self.buf.push(bytes.len() as u8);
            self.buf.extend_from_slice(bytes);
        }
        self.buf.push(0);
        Ok(())
    }

    fn encode_question(&mut self, q: &DNSQuestion) -> Result<(), String> {
        self.encode_name(&q.name)?;
        self.buf.extend_from_slice(&q.qtype.to_u16().to_be_bytes());
        self.buf.extend_from_slice(&q.qclass.to_u16().to_be_bytes());
        Ok(())
    }

    fn encode_record(&mut self, record: &DNSRecord) -> Result<(), String> {
        self.encode_name(&record.name)?;
        self.buf
            .extend_from_slice(&record.rtype.to_u16().to_be_bytes());
        self.buf
            .extend_from_slice(&record.rclass.to_u16().to_be_bytes());
        self.buf.extend_from_slice(&record.ttl.to_be_bytes());

        let rdlength_pos = self.buf.len();
        self.buf.extend_from_slice(&[0, 0]);

        let data_start = self.buf.len();
        self.encode_rdata(&record.rdata)?;
        let data_len = (self.buf.len() - data_start) as u16;

        self.buf[rdlength_pos..rdlength_pos + 2].copy_from_slice(&data_len.to_be_bytes());
        Ok(())
    }

    fn encode_rdata(&mut self, rdata: &RData) -> Result<(), String> {
        match rdata {
            RData::A(ip) => {
                let bytes = ip.to_bytes();
                if bytes.len() == 4 {
                    self.buf.extend_from_slice(&bytes);
                } else {
                    return Err("Invalid IPv4 address for A record".to_string());
                }
            }
            RData::AAAA(ip) => {
                let bytes = ip.to_bytes();
                if bytes.len() == 16 {
                    self.buf.extend_from_slice(&bytes);
                } else {
                    return Err("Invalid IPv6 address for AAAA record".to_string());
                }
            }
            RData::CNAME(target) => self.encode_name(target)?,
            RData::NS(ns) => self.encode_name(ns)?,
            RData::PTR(ptr) => self.encode_name(ptr)?,
            RData::MX { priority, exchange } => {
                self.buf.extend_from_slice(&priority.to_be_bytes());
                self.encode_name(exchange)?;
            }
            RData::TXT(strings) => {
                for s in strings {
                    let b = s.as_bytes();
                    if b.len() > 255 {
                        return Err("TXT string segment exceeds 255 bytes".to_string());
                    }
                    self.buf.push(b.len() as u8);
                    self.buf.extend_from_slice(b);
                }
            }
            RData::Unknown { data, .. } => {
                self.buf.extend_from_slice(data);
            }
            _ => {
                // Fallback for unhandled complex RData types in basic encoder
            }
        }
        Ok(())
    }
}

pub struct DNSWireDecoder<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> DNSWireDecoder<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn decode(buf: &'a [u8]) -> Result<DNSMessage, String> {
        let mut decoder = Self::new(buf);
        let header = decoder.decode_header()?;

        let mut questions = Vec::with_capacity(header.qdcount as usize);
        for _ in 0..header.qdcount {
            questions.push(decoder.decode_question()?);
        }

        let mut answers = Vec::with_capacity(header.ancount as usize);
        for _ in 0..header.ancount {
            answers.push(decoder.decode_record()?);
        }

        let mut authorities = Vec::with_capacity(header.nscount as usize);
        for _ in 0..header.nscount {
            authorities.push(decoder.decode_record()?);
        }

        let mut additionals = Vec::with_capacity(header.arcount as usize);
        for _ in 0..header.arcount {
            additionals.push(decoder.decode_record()?);
        }

        Ok(DNSMessage {
            header,
            questions,
            answers,
            authorities,
            additionals,
        })
    }

    fn read_u8(&mut self) -> Result<u8, String> {
        if self.pos >= self.buf.len() {
            return Err("Unexpected end of DNS packet buffer".to_string());
        }
        let b = self.buf[self.pos];
        self.pos += 1;
        Ok(b)
    }

    fn read_u16(&mut self) -> Result<u16, String> {
        if self.pos + 2 > self.buf.len() {
            return Err("Unexpected end of DNS packet buffer".to_string());
        }
        let val = u16::from_be_bytes([self.buf[self.pos], self.buf[self.pos + 1]]);
        self.pos += 2;
        Ok(val)
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        if self.pos + 4 > self.buf.len() {
            return Err("Unexpected end of DNS packet buffer".to_string());
        }
        let val = u32::from_be_bytes([
            self.buf[self.pos],
            self.buf[self.pos + 1],
            self.buf[self.pos + 2],
            self.buf[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(val)
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], String> {
        if self.pos + len > self.buf.len() {
            return Err("Unexpected end of DNS packet buffer".to_string());
        }
        let slice = &self.buf[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    fn decode_header(&mut self) -> Result<DNSHeader, String> {
        let id = self.read_u16()?;
        let flags = self.read_u16()?;

        let qr = (flags & (1 << 15)) != 0;
        let opcode = ((flags >> 11) & 0x0F) as u8;
        let aa = (flags & (1 << 10)) != 0;
        let tc = (flags & (1 << 9)) != 0;
        let rd = (flags & (1 << 8)) != 0;
        let ra = (flags & (1 << 7)) != 0;
        let z = (flags & (1 << 6)) != 0;
        let ad = (flags & (1 << 5)) != 0;
        let cd = (flags & (1 << 4)) != 0;
        let rcode = (flags & 0x0F) as u8;

        let qdcount = self.read_u16()?;
        let ancount = self.read_u16()?;
        let nscount = self.read_u16()?;
        let arcount = self.read_u16()?;

        Ok(DNSHeader {
            id,
            qr,
            opcode,
            aa,
            tc,
            rd,
            ra,
            z,
            ad,
            cd,
            rcode,
            qdcount,
            ancount,
            nscount,
            arcount,
        })
    }

    fn decode_name(&mut self) -> Result<DNSName, String> {
        let mut labels: Vec<String> = Vec::new();
        let mut current_pos = self.pos;
        let mut jumped = false;
        let mut depth = 0;

        loop {
            if depth > MAX_POINTER_DEPTH {
                return Err("DNS compression pointer loop detected".to_string());
            }

            if current_pos >= self.buf.len() {
                return Err("Unexpected EOF while parsing domain name".to_string());
            }

            let len = self.buf[current_pos];

            if (len & 0xC0) == 0xC0 {
                if current_pos + 1 >= self.buf.len() {
                    return Err("Truncated compression pointer in domain name".to_string());
                }
                let ptr_bytes =
                    u16::from_be_bytes([self.buf[current_pos], self.buf[current_pos + 1]]);
                let offset = (ptr_bytes & 0x3FFF) as usize;

                if !jumped {
                    self.pos = current_pos + 2;
                    jumped = true;
                }

                current_pos = offset;
                depth += 1;
                continue;
            } else if (len & 0xC0) != 0 {
                return Err(format!("Invalid label type tag 0x{:X}", len & 0xC0));
            }

            current_pos += 1;

            if len == 0 {
                if !jumped {
                    self.pos = current_pos;
                }
                break;
            }

            let label_len = len as usize;
            if current_pos + label_len > self.buf.len() {
                return Err("Unexpected EOF reading domain label".to_string());
            }

            let label_bytes = &self.buf[current_pos..current_pos + label_len];
            let label_str = std::str::from_utf8(label_bytes)
                .map_err(|_| "Invalid UTF-8 in DNS label".to_string())?;

            labels.push(label_str.to_string());
            current_pos += label_len;
        }

        if labels.is_empty() {
            Ok(DNSName::root())
        } else {
            DNSName::parse(&labels.join("."))
        }
    }

    fn decode_question(&mut self) -> Result<DNSQuestion, String> {
        let name = self.decode_name()?;
        let qtype = DNSType::from_u16(self.read_u16()?);
        let qclass = DNSClass::from_u16(self.read_u16()?);

        Ok(DNSQuestion {
            name,
            qtype,
            qclass,
        })
    }

    fn decode_record(&mut self) -> Result<DNSRecord, String> {
        let name = self.decode_name()?;
        let rtype = DNSType::from_u16(self.read_u16()?);
        let rclass = DNSClass::from_u16(self.read_u16()?);
        let ttl = self.read_u32()?;
        let rdlength = self.read_u16()? as usize;

        let rdata_start = self.pos;
        let rdata_bytes = self.read_bytes(rdlength)?;

        let rdata = match rtype {
            DNSType::A => {
                if rdata_bytes.len() == 4 {
                    let ip_str = format!(
                        "{}.{}.{}.{}",
                        rdata_bytes[0], rdata_bytes[1], rdata_bytes[2], rdata_bytes[3]
                    );
                    let ip = IPAddress::parse(&ip_str)?;
                    RData::A(ip)
                } else {
                    RData::Unknown {
                        type_code: rtype.to_u16(),
                        data: rdata_bytes.to_vec(),
                    }
                }
            }
            DNSType::AAAA => {
                if rdata_bytes.len() == 16 {
                    let mut segments = Vec::new();
                    for i in 0..8 {
                        let seg = u16::from_be_bytes([rdata_bytes[i * 2], rdata_bytes[i * 2 + 1]]);
                        segments.push(format!("{:x}", seg));
                    }
                    let ip_str = segments.join(":");
                    let ip = IPAddress::parse(&ip_str)?;
                    RData::AAAA(ip)
                } else {
                    RData::Unknown {
                        type_code: rtype.to_u16(),
                        data: rdata_bytes.to_vec(),
                    }
                }
            }
            DNSType::CNAME => {
                let mut full_decoder = DNSWireDecoder::new(self.buf);
                full_decoder.pos = rdata_start;
                let target = full_decoder.decode_name()?;
                RData::CNAME(target)
            }
            DNSType::NS => {
                let mut full_decoder = DNSWireDecoder::new(self.buf);
                full_decoder.pos = rdata_start;
                let ns = full_decoder.decode_name()?;
                RData::NS(ns)
            }
            DNSType::PTR => {
                let mut full_decoder = DNSWireDecoder::new(self.buf);
                full_decoder.pos = rdata_start;
                let ptr = full_decoder.decode_name()?;
                RData::PTR(ptr)
            }
            DNSType::MX => {
                let mut full_decoder = DNSWireDecoder::new(self.buf);
                full_decoder.pos = rdata_start;
                let priority = full_decoder.read_u16()?;
                let exchange = full_decoder.decode_name()?;
                RData::MX { priority, exchange }
            }
            DNSType::SRV => {
                let mut full_decoder = DNSWireDecoder::new(self.buf);
                full_decoder.pos = rdata_start;
                let priority = full_decoder.read_u16()?;
                let weight = full_decoder.read_u16()?;
                let port = full_decoder.read_u16()?;
                let target = full_decoder.decode_name()?;
                RData::SRV {
                    priority,
                    weight,
                    port,
                    target,
                }
            }
            DNSType::CAA => {
                if rdata_bytes.len() >= 2 {
                    let flags = rdata_bytes[0];
                    let tag_len = rdata_bytes[1] as usize;
                    if 2 + tag_len <= rdata_bytes.len() {
                        let tag = String::from_utf8_lossy(&rdata_bytes[2..2 + tag_len]).to_string();
                        let value =
                            String::from_utf8_lossy(&rdata_bytes[2 + tag_len..]).to_string();
                        RData::CAA { flags, tag, value }
                    } else {
                        RData::Unknown {
                            type_code: rtype.to_u16(),
                            data: rdata_bytes.to_vec(),
                        }
                    }
                } else {
                    RData::Unknown {
                        type_code: rtype.to_u16(),
                        data: rdata_bytes.to_vec(),
                    }
                }
            }
            DNSType::SOA => {
                let mut full_decoder = DNSWireDecoder::new(self.buf);
                full_decoder.pos = rdata_start;
                let mname = full_decoder.decode_name()?;
                let rname = full_decoder.decode_name()?;
                let serial = full_decoder.read_u32()?;
                let refresh = full_decoder.read_u32()?;
                let retry = full_decoder.read_u32()?;
                let expire = full_decoder.read_u32()?;
                let minimum = full_decoder.read_u32()?;
                RData::SOA {
                    mname,
                    rname,
                    serial,
                    refresh,
                    retry,
                    expire,
                    minimum,
                }
            }
            DNSType::TXT => {
                let mut strings = Vec::new();
                let mut pos = 0;
                while pos < rdata_bytes.len() {
                    let len = rdata_bytes[pos] as usize;
                    pos += 1;
                    if pos + len <= rdata_bytes.len() {
                        if let Ok(s) = std::str::from_utf8(&rdata_bytes[pos..pos + len]) {
                            strings.push(s.to_string());
                        }
                        pos += len;
                    } else {
                        break;
                    }
                }
                RData::TXT(strings)
            }
            _ => RData::Unknown {
                type_code: rtype.to_u16(),
                data: rdata_bytes.to_vec(),
            },
        };

        Ok(DNSRecord {
            name,
            rtype,
            rclass,
            ttl,
            rdata,
        })
    }
}
