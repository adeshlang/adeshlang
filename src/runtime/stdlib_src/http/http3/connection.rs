use super::super::body::Body;
use super::super::errors::HttpError;
use super::super::headers::Headers;
use super::super::request::Request;
use super::super::response::Response;
use super::super::status::HttpStatus;
use super::super::version::HttpVersion;
use super::frames::Http3Frame;
use super::qpack::{QpackDecoder, QpackEncoder};
use super::streams::Http3Stream;
use crate::utils::collections::FastMap;

pub struct Http3Connection {
    pub is_client: bool,
    pub next_stream_id: u64,
    pub active_streams: FastMap<u64, Http3Stream>,
    pub encoder: QpackEncoder,
    pub decoder: QpackDecoder,
    pub settings_sent: bool,
}

impl Http3Connection {
    pub fn new(is_client: bool) -> Self {
        Self {
            is_client,
            next_stream_id: if is_client { 0 } else { 1 },
            active_streams: FastMap::default(),
            encoder: QpackEncoder::new(4096),
            decoder: QpackDecoder::new(4096),
            settings_sent: false,
        }
    }

    pub fn encode_request_stream(&mut self, req: &Request) -> Result<(u64, Vec<u8>), HttpError> {
        let stream_id = self.next_stream_id;
        self.next_stream_id += 4; // Client-initiated bidirectional stream increment

        let path = req.uri.path_and_query();
        let scheme = if req.uri.is_https() { "https" } else { "http" };
        let authority = req.uri.host.clone().unwrap_or_default();

        let mut headers = vec![
            (":method", req.method.as_str()),
            (":scheme", scheme),
            (":path", path.as_str()),
            (":authority", authority.as_str()),
        ];

        let req_headers_map = req.headers.to_map();
        for (k, v) in &req_headers_map {
            if k != "host" && k != "connection" {
                headers.push((k.as_str(), v.as_str()));
            }
        }

        let encoded_headers = self.encoder.encode(&headers);
        let header_frame = Http3Frame::Headers {
            encoded_fields: encoded_headers,
        };

        let mut wire = header_frame.encode();
        let body_bytes = req.body.to_bytes()?;
        if !body_bytes.is_empty() {
            let data_frame = Http3Frame::Data { data: body_bytes };
            wire.extend_from_slice(&data_frame.encode());
        }

        self.active_streams
            .insert(stream_id, Http3Stream::new(stream_id));
        Ok((stream_id, wire))
    }

    pub fn encode_response_headers(
        &mut self,
        stream_id: u64,
        resp: &Response,
    ) -> Result<Vec<u8>, HttpError> {
        let status_str = resp.status.code().to_string();
        let mut headers = vec![(":status", status_str.as_str())];

        let resp_headers_map = resp.headers.to_map();
        for (k, v) in &resp_headers_map {
            if !k.starts_with(':') {
                headers.push((k.as_str(), v.as_str()));
            }
        }

        let encoded_headers = self.encoder.encode(&headers);
        let header_frame = Http3Frame::Headers {
            encoded_fields: encoded_headers,
        };

        let stream = self
            .active_streams
            .entry(stream_id)
            .or_insert_with(|| Http3Stream::new(stream_id));
        stream.mark_headers_sent();

        Ok(header_frame.encode())
    }

    pub fn encode_response_data(
        &mut self,
        stream_id: u64,
        data: &[u8],
    ) -> Result<Vec<u8>, HttpError> {
        if data.is_empty() {
            return Ok(Vec::new());
        }
        let data_frame = Http3Frame::Data {
            data: data.to_vec(),
        };
        let stream = self
            .active_streams
            .entry(stream_id)
            .or_insert_with(|| Http3Stream::new(stream_id));
        stream.mark_body_streaming();
        Ok(data_frame.encode())
    }

    pub fn encode_response_trailers(
        &mut self,
        _stream_id: u64,
        trailers: &Headers,
    ) -> Result<Vec<u8>, HttpError> {
        let map = trailers.to_map();
        let mut trailer_tuples = Vec::new();
        for (k, v) in &map {
            trailer_tuples.push((k.as_str(), v.as_str()));
        }
        let encoded_trailers = self.encoder.encode(&trailer_tuples);
        let trailer_frame = Http3Frame::Headers {
            encoded_fields: encoded_trailers,
        };
        Ok(trailer_frame.encode())
    }

    pub fn encode_full_response(
        &mut self,
        stream_id: u64,
        resp: &Response,
        is_head: bool,
    ) -> Result<Vec<u8>, HttpError> {
        let mut wire = Vec::new();

        // 1. HEADERS frame
        wire.extend(self.encode_response_headers(stream_id, resp)?);

        // 2. DATA frame (skip if HEAD request, 204 No Content, or 304 Not Modified)
        let code = resp.status.code();
        let skip_body = is_head || code == 204 || code == 304;
        if !skip_body {
            let body_bytes = resp.body.to_bytes().unwrap_or_default();
            if !body_bytes.is_empty() {
                wire.extend(self.encode_response_data(stream_id, &body_bytes)?);
            }
        }

        // 3. TRAILERS frame (if any)
        if let Some(trailers) = &resp.trailers {
            wire.extend(self.encode_response_trailers(stream_id, trailers)?);
        }

        // 4. Mark completed
        let stream = self
            .active_streams
            .entry(stream_id)
            .or_insert_with(|| Http3Stream::new(stream_id));
        stream.mark_completed();

        Ok(wire)
    }

    pub fn encode_response_stream(&mut self, resp: &Response) -> Result<Vec<u8>, HttpError> {
        self.encode_full_response(0, resp, false)
    }

    pub fn process_incoming_stream_data(
        &mut self,
        stream_id: u64,
        data: &[u8],
    ) -> Result<Option<Response>, HttpError> {
        let mut offset = 0;
        let mut resp_opt = None;

        while offset < data.len() {
            if let Some((frame, consumed)) = Http3Frame::decode(&data[offset..])? {
                offset += consumed;
                match frame {
                    Http3Frame::Headers { encoded_fields } => {
                        let decoded = self.decoder.decode(&encoded_fields)?;
                        let stream = self
                            .active_streams
                            .entry(stream_id)
                            .or_insert_with(|| Http3Stream::new(stream_id));
                        let is_trailers = !stream.incoming_headers.is_empty();
                        if is_trailers {
                            let trailers = stream.trailers.get_or_insert_with(Headers::new);
                            for (name, val) in decoded {
                                if !name.starts_with(':') {
                                    let _ = trailers.append(&name, &val);
                                }
                            }
                        } else {
                            stream.incoming_headers.extend(decoded);
                        }
                    }
                    Http3Frame::Data { data } => {
                        let stream = self
                            .active_streams
                            .entry(stream_id)
                            .or_insert_with(|| Http3Stream::new(stream_id));
                        stream.incoming_data.extend_from_slice(&data);
                    }
                    _ => {}
                }
            } else {
                break;
            }
        }

        if let Some(stream) = self.active_streams.get(&stream_id) {
            let mut status = HttpStatus::OK;
            let mut headers = Headers::new();
            for (name, val) in &stream.incoming_headers {
                if name == ":status" {
                    let code = val.parse::<u16>().unwrap_or(200);
                    status = HttpStatus(code);
                } else if !name.starts_with(':') {
                    let _ = headers.append(name, val);
                }
            }
            let resp = Response {
                status,
                version: HttpVersion::Http30,
                headers,
                body: Body::from_bytes(stream.incoming_data.clone()),
                trailers: stream.trailers.clone(),
            };
            resp_opt = Some(resp);
        }

        Ok(resp_opt)
    }
}
