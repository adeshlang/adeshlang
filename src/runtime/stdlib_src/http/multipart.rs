use super::errors::HttpError;

#[derive(Debug, Clone)]
pub struct MultipartPart {
    pub name: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct MultipartForm {
    pub boundary: String,
    pub parts: Vec<MultipartPart>,
}

impl Default for MultipartForm {
    fn default() -> Self {
        Self::new()
    }
}

impl MultipartForm {
    pub fn new() -> Self {
        let boundary = format!("---------------------------{}", rand::random::<u64>());
        Self {
            boundary,
            parts: Vec::new(),
        }
    }

    pub fn add_field(&mut self, name: &str, value: &str) {
        self.parts.push(MultipartPart {
            name: name.to_string(),
            filename: None,
            content_type: Some("text/plain; charset=utf-8".to_string()),
            data: value.as_bytes().to_vec(),
        });
    }

    pub fn add_file(&mut self, name: &str, filename: &str, content_type: &str, data: Vec<u8>) {
        self.parts.push(MultipartPart {
            name: name.to_string(),
            filename: Some(filename.to_string()),
            content_type: Some(content_type.to_string()),
            data,
        });
    }

    pub fn encode(&self) -> (Vec<u8>, String) {
        let mut out = Vec::new();
        let content_type = format!("multipart/form-data; boundary={}", self.boundary);

        for part in &self.parts {
            out.extend_from_slice(format!("--{}\r\n", self.boundary).as_bytes());
            if let Some(ref fname) = part.filename {
                out.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                        part.name, fname
                    )
                    .as_bytes(),
                );
            } else {
                out.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{}\"\r\n", part.name)
                        .as_bytes(),
                );
            }

            if let Some(ref ct) = part.content_type {
                out.extend_from_slice(format!("Content-Type: {}\r\n", ct).as_bytes());
            }

            out.extend_from_slice(b"\r\n");
            out.extend_from_slice(&part.data);
            out.extend_from_slice(b"\r\n");
        }

        out.extend_from_slice(format!("--{}--\r\n", self.boundary).as_bytes());

        (out, content_type)
    }

    pub fn parse(body: &[u8], boundary: &str) -> Result<Vec<MultipartPart>, HttpError> {
        let sep = format!("--{}", boundary);
        let body_str = String::from_utf8_lossy(body);
        let mut parts = Vec::new();

        for chunk in body_str.split(&sep) {
            let trimmed = chunk.trim_matches(|c| c == '\r' || c == '\n' || c == '-');
            if trimmed.is_empty() {
                continue;
            }

            if let Some(hdr_end) = trimmed.find("\r\n\r\n") {
                let hdrs = &trimmed[..hdr_end];
                let content = &trimmed[hdr_end + 4..];

                let mut name = "unnamed".to_string();
                let mut filename = None;
                let mut content_type = None;

                for line in hdrs.lines() {
                    let l_lower = line.to_ascii_lowercase();
                    if l_lower.starts_with("content-disposition:") {
                        if let Some(n_pos) = line.find("name=\"") {
                            let n_end = line[n_pos + 6..].find('"').unwrap_or(0);
                            name = line[n_pos + 6..n_pos + 6 + n_end].to_string();
                        }
                        if let Some(f_pos) = line.find("filename=\"") {
                            let f_end = line[f_pos + 10..].find('"').unwrap_or(0);
                            filename = Some(line[f_pos + 10..f_pos + 10 + f_end].to_string());
                        }
                    } else if l_lower.starts_with("content-type:") {
                        content_type = Some(line[13..].trim().to_string());
                    }
                }

                parts.push(MultipartPart {
                    name,
                    filename,
                    content_type,
                    data: content.as_bytes().to_vec(),
                });
            }
        }

        Ok(parts)
    }
}
