//! IDNA (Internationalized Domain Names in Applications) & Punycode for AdeshLang URL.
//! Implements RFC 5890, RFC 5891, RFC 3492 Punycode algorithms and homograph analysis.

const INITIAL_N: u32 = 128;
const INITIAL_BIAS: u32 = 72;
const DAMP: u32 = 700;
const SBASE: u32 = 36;
const TMIN: u32 = 1;
const TMAX: u32 = 26;
const BASE: u32 = 36;
const ACE_PREFIX: &str = "xn--";

/// Encode a Unicode domain string into ASCII IDNA (Punycode) representation.
pub fn domain_to_ascii(domain: &str) -> Result<String, String> {
    if domain.is_empty() {
        return Ok(String::new());
    }

    let mut ascii_parts = Vec::new();
    for label in domain.split('.') {
        if label.is_empty() {
            ascii_parts.push(String::new());
            continue;
        }

        if label.is_ascii() {
            ascii_parts.push(label.to_lowercase());
        } else {
            let encoded = encode_punycode_label(label)?;
            ascii_parts.push(format!("{}{}", ACE_PREFIX, encoded));
        }
    }

    Ok(ascii_parts.join("."))
}

/// Decode an ASCII IDNA Punycode domain string back into Unicode representation.
pub fn domain_to_unicode(domain: &str) -> Result<String, String> {
    if domain.is_empty() {
        return Ok(String::new());
    }

    let mut unicode_parts = Vec::new();
    for label in domain.split('.') {
        let lower = label.to_lowercase();
        if lower.starts_with(ACE_PREFIX) {
            let puny = &lower[ACE_PREFIX.len()..];
            let decoded = decode_punycode_label(puny)?;
            unicode_parts.push(decoded);
        } else {
            unicode_parts.push(label.to_string());
        }
    }

    Ok(unicode_parts.join("."))
}

/// Encode a single non-ASCII Unicode label using RFC 3492 Punycode.
fn encode_punycode_label(input: &str) -> Result<String, String> {
    let input_chars: Vec<u32> = input.chars().map(|c| c as u32).collect();
    let mut output = String::new();

    // Copy basic ASCII characters
    for &code in &input_chars {
        if code < INITIAL_N {
            output.push((code as u8) as char);
        }
    }

    let basic_count = output.len() as u32;
    if basic_count > 0 {
        output.push('-');
    }

    let mut n = INITIAL_N;
    let mut delta = 0u32;
    let mut bias = INITIAL_BIAS;
    let mut handled_count = basic_count;
    let total_count = input_chars.len() as u32;

    while handled_count < total_count {
        // Find next smallest code point >= n
        let mut m = u32::MAX;
        for &code in &input_chars {
            if code >= n && code < m {
                m = code;
            }
        }

        delta = delta
            .checked_add(
                (m - n)
                    .checked_mul(handled_count + 1)
                    .ok_or("Overflow in Punycode delta calculation")?,
            )
            .ok_or("Overflow in Punycode delta calculation")?;
        n = m;

        for &code in &input_chars {
            if code < n {
                delta = delta.checked_add(1).ok_or("Overflow in Punycode delta")?;
            } else if code == n {
                let mut q = delta;
                let mut k = BASE;

                loop {
                    let t = if k <= bias {
                        TMIN
                    } else if k >= bias + TMAX {
                        TMAX
                    } else {
                        k - bias
                    };

                    if q < t {
                        output.push(encode_digit(q));
                        break;
                    }

                    output.push(encode_digit(t + (q - t) % (BASE - t)));
                    q = (q - t) / (BASE - t);
                    k += BASE;
                }

                bias = adapt(delta, handled_count + 1, handled_count == basic_count);
                delta = 0;
                handled_count += 1;
            }
        }

        delta += 1;
        n += 1;
    }

    Ok(output)
}

/// Decode a single Punycode label into a Unicode string.
fn decode_punycode_label(input: &str) -> Result<String, String> {
    let mut output: Vec<u32> = Vec::new();

    // Find delimiter '-' from right
    if let Some(delim_pos) = input.rfind('-') {
        for &b in &input.as_bytes()[..delim_pos] {
            if b >= INITIAL_N as u8 {
                return Err("Invalid basic character in Punycode label".to_string());
            }
            output.push(b as u32);
        }
    }

    let mut n = INITIAL_N;
    let mut i = 0u32;
    let mut bias = INITIAL_BIAS;

    let input_slice = if let Some(delim) = input.rfind('-') {
        &input[delim + 1..]
    } else {
        input
    };

    let mut input_chars = input_slice.chars().peekable();

    while input_chars.peek().is_some() {
        let oldi = i;
        let mut w = 1u32;
        let mut k = BASE;

        loop {
            let ch = input_chars
                .next()
                .ok_or_else(|| "Unexpected end of Punycode string".to_string())?;
            let digit = decode_digit(ch)?;

            i = i
                .checked_add(digit.checked_mul(w).ok_or("Overflow in Punycode decode")?)
                .ok_or("Overflow in Punycode decode")?;

            let t = if k <= bias {
                TMIN
            } else if k >= bias + TMAX {
                TMAX
            } else {
                k - bias
            };

            if digit < t {
                break;
            }

            w = w
                .checked_mul(BASE - t)
                .ok_or("Overflow in Punycode weight calculation")?;
            k += BASE;
        }

        let len = (output.len() + 1) as u32;
        bias = adapt(i - oldi, len, oldi == 0);
        n = n
            .checked_add(i / len)
            .ok_or("Overflow in Punycode code point calculation")?;
        i %= len;

        if i as usize > output.len() {
            return Err("Invalid insertion index in Punycode decode".to_string());
        }

        output.insert(i as usize, n);
        i += 1;
    }

    let mut res = String::new();
    for &code in &output {
        if let Some(ch) = std::char::from_u32(code) {
            res.push(ch);
        } else {
            return Err(format!("Invalid Unicode code point U+{:X}", code));
        }
    }

    Ok(res)
}

fn adapt(mut delta: u32, numpoints: u32, firsttime: bool) -> u32 {
    delta = if firsttime { delta / DAMP } else { delta / 2 };

    delta += delta / numpoints;
    let mut k = 0;

    while delta > ((BASE - TMIN) * TMAX) / 2 {
        delta /= BASE - TMIN;
        k += BASE;
    }

    k + (((BASE - TMIN + 1) * delta) / (delta + SBASE))
}

fn encode_digit(d: u32) -> char {
    if d < 26 {
        (b'a' + d as u8) as char
    } else if d < 36 {
        (b'0' + (d - 26) as u8) as char
    } else {
        '?'
    }
}

fn decode_digit(c: char) -> Result<u32, String> {
    match c {
        'a'..='z' => Ok((c as u32) - ('a' as u32)),
        'A'..='Z' => Ok((c as u32) - ('A' as u32)),
        '0'..='9' => Ok((c as u32) - ('0' as u32) + 26),
        _ => Err(format!("Invalid Punycode character '{}'", c)),
    }
}

/// Advisory homograph & confusable risk analysis.
pub fn is_suspicious_domain(domain: &str) -> bool {
    // Check for mixed scripts or common Cyrillic/Greek lookalikes mixed with Latin
    let has_ascii = domain.chars().any(|c| c.is_ascii_alphanumeric());
    let has_non_ascii = domain.chars().any(|c| !c.is_ascii() && c != '.');

    if has_ascii && has_non_ascii {
        // Mixed script detected
        return true;
    }

    // Check for punycode prefix
    if domain.to_lowercase().contains(ACE_PREFIX) {
        return true;
    }

    false
}
