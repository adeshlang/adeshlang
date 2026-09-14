use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::Engine;
use sha2::{Digest as ShaDigestTrait, Sha256, Sha512, Sha512_256};

use super::body::{Body, BodyStreamCallback};
use super::errors::{HttpError, HttpErrorKind};
use super::request::Request;
use super::response::Response;
use super::structured_fields::{
    StructuredDictionary, StructuredFields, StructuredItem, StructuredParameters, StructuredValue,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DigestAlgorithm {
    Sha256,
    Sha512,
    Sha512_256,
    Blake3,
}

impl DigestAlgorithm {
    pub fn from_token(token: &str) -> Result<Self, HttpError> {
        match token.trim().to_ascii_lowercase().as_str() {
            "sha-256" => Ok(Self::Sha256),
            "sha-512" => Ok(Self::Sha512),
            "sha-512-256" => Ok(Self::Sha512_256),
            "blake3" => Ok(Self::Blake3),
            other => Err(HttpError::new(
                HttpErrorKind::ProtocolError,
                format!("Unsupported digest algorithm: {}", other),
            )),
        }
    }

    pub fn token(&self) -> &'static str {
        match self {
            Self::Sha256 => "sha-256",
            Self::Sha512 => "sha-512",
            Self::Sha512_256 => "sha-512-256",
            Self::Blake3 => "blake3",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DigestPreference {
    pub weights: HashMap<DigestAlgorithm, u16>,
}

impl DigestPreference {
    pub fn new() -> Self {
        Self {
            weights: HashMap::new(),
        }
    }

    pub fn with(mut self, alg: DigestAlgorithm, weight: u16) -> Self {
        self.weights.insert(alg, weight.min(1000));
        self
    }

    pub fn encode(&self) -> String {
        let mut entries = HashMap::new();
        for (alg, weight) in &self.weights {
            entries.insert(
                alg.token().to_string(),
                (
                    StructuredValue::Item(StructuredItem::Integer(*weight as i64)),
                    StructuredParameters::new(),
                ),
            );
        }

        StructuredFields::encode_dictionary(&StructuredDictionary { entries })
    }

    pub fn parse(raw: &str) -> Result<Self, HttpError> {
        let dict = StructuredFields::parse_dictionary(raw)?;
        let mut pref = DigestPreference::new();
        for (key, (value, _)) in dict.entries {
            let alg = DigestAlgorithm::from_token(&key)?;
            let weight = match value {
                StructuredValue::Item(StructuredItem::Integer(v)) => v,
                _ => {
                    return Err(HttpError::new(
                        HttpErrorKind::ParseError,
                        "Want-* digest preference values must be integers",
                    ));
                }
            };

            if !(0..=1000).contains(&weight) {
                return Err(HttpError::new(
                    HttpErrorKind::ParseError,
                    "Want-* digest preference values must be in [0, 1000]",
                ));
            }

            pref.weights.insert(alg, weight as u16);
        }
        Ok(pref)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ContentDigest {
    pub values: HashMap<DigestAlgorithm, Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub struct RepresentationDigest {
    pub values: HashMap<DigestAlgorithm, Vec<u8>>,
}

impl ContentDigest {
    pub fn encode(&self) -> String {
        encode_digest_map(&self.values)
    }

    pub fn parse(raw: &str) -> Result<Self, HttpError> {
        Ok(Self {
            values: parse_digest_map(raw)?,
        })
    }
}

impl RepresentationDigest {
    pub fn encode(&self) -> String {
        encode_digest_map(&self.values)
    }

    pub fn parse(raw: &str) -> Result<Self, HttpError> {
        Ok(Self {
            values: parse_digest_map(raw)?,
        })
    }
}

pub fn compute_digest_bytes(algorithm: DigestAlgorithm, data: &[u8]) -> Vec<u8> {
    match algorithm {
        DigestAlgorithm::Sha256 => Sha256::digest(data).to_vec(),
        DigestAlgorithm::Sha512 => Sha512::digest(data).to_vec(),
        DigestAlgorithm::Sha512_256 => Sha512_256::digest(data).to_vec(),
        DigestAlgorithm::Blake3 => blake3::hash(data).as_bytes().to_vec(),
    }
}

pub fn verify_content_digest(
    response: &Response,
    algorithm: DigestAlgorithm,
) -> Result<(), HttpError> {
    let expected = expected_digest_for_response(response, "content-digest", algorithm)?;
    let bytes = response.body.to_bytes()?;
    let actual = compute_digest_bytes(algorithm, &bytes);
    if actual == expected {
        Ok(())
    } else {
        Err(HttpError::new(
            HttpErrorKind::SecurityViolation,
            format!(
                "Content-Digest verification failed for {}",
                algorithm.token()
            ),
        ))
    }
}

pub fn verify_representation_digest(
    response: &Response,
    algorithm: DigestAlgorithm,
) -> Result<(), HttpError> {
    let expected = expected_digest_for_response(response, "repr-digest", algorithm)?;
    let bytes = response.body.to_bytes()?;
    let actual = compute_digest_bytes(algorithm, &bytes);
    if actual == expected {
        Ok(())
    } else {
        Err(HttpError::new(
            HttpErrorKind::SecurityViolation,
            format!("Repr-Digest verification failed for {}", algorithm.token()),
        ))
    }
}

pub fn wrap_response_stream_for_digest_verification(
    response: &mut Response,
    algorithm: DigestAlgorithm,
) -> Result<(), HttpError> {
    let expected = expected_digest_for_response(response, "content-digest", algorithm)?;
    let stream_cb = match &response.body {
        Body::Stream(cb) => cb.clone(),
        Body::Bytes(bytes) => {
            let actual = compute_digest_bytes(algorithm, bytes);
            if actual == expected {
                return Ok(());
            }
            return Err(HttpError::new(
                HttpErrorKind::SecurityViolation,
                format!(
                    "Content-Digest verification failed for {}",
                    algorithm.token()
                ),
            ));
        }
        Body::Empty => {
            let actual = compute_digest_bytes(algorithm, &[]);
            if actual == expected {
                return Ok(());
            }
            return Err(HttpError::new(
                HttpErrorKind::SecurityViolation,
                format!(
                    "Content-Digest verification failed for {}",
                    algorithm.token()
                ),
            ));
        }
        Body::AsyncStream(s) => {
            let bytes = s.read_to_end()?;
            let actual = compute_digest_bytes(algorithm, &bytes);
            if actual == expected {
                return Ok(());
            }
            return Err(HttpError::new(
                HttpErrorKind::SecurityViolation,
                format!(
                    "Content-Digest verification failed for {}",
                    algorithm.token()
                ),
            ));
        }
    };

    let verifier = Arc::new(Mutex::new(StreamingDigestVerifier::new(
        algorithm, expected,
    )));
    response.body = Body::Stream(make_verifying_stream(stream_cb, verifier));
    Ok(())
}

pub fn apply_content_digest_header(
    req: &mut Request,
    algorithm: DigestAlgorithm,
) -> Result<(), HttpError> {
    match &req.body {
        Body::Stream(_) => {
            return Err(HttpError::new(
                HttpErrorKind::ProtocolError,
                "Cannot precompute Content-Digest for streaming request body",
            ));
        }
        _ => {}
    }

    let bytes = req.body.to_bytes()?;
    let digest = compute_digest_bytes(algorithm, &bytes);
    let mut values = HashMap::new();
    values.insert(algorithm, digest);
    req.headers
        .insert("content-digest", &encode_digest_map(&values))
}

pub fn apply_repr_digest_header(
    resp: &mut Response,
    algorithm: DigestAlgorithm,
) -> Result<(), HttpError> {
    match &resp.body {
        Body::Stream(_) => {
            return Err(HttpError::new(
                HttpErrorKind::ProtocolError,
                "Cannot precompute Repr-Digest for streaming response body",
            ));
        }
        _ => {}
    }

    let bytes = resp.body.to_bytes()?;
    let digest = compute_digest_bytes(algorithm, &bytes);
    let mut values = HashMap::new();
    values.insert(algorithm, digest);
    resp.headers
        .insert("repr-digest", &encode_digest_map(&values))
}

pub fn apply_content_digest_trailer(
    resp: &mut Response,
    algorithm: DigestAlgorithm,
) -> Result<(), HttpError> {
    match &resp.body {
        Body::Stream(_) => {
            return Err(HttpError::new(
                HttpErrorKind::ProtocolError,
                "Automatic digest trailers for streaming response require an explicit trailer writer",
            ));
        }
        _ => {}
    }

    let digest = compute_digest_bytes(algorithm, &resp.body.to_bytes()?);
    let mut values = HashMap::new();
    values.insert(algorithm, digest);

    if !resp.headers.contains("trailer") {
        resp.headers.insert("trailer", "Content-Digest")?;
    }

    let mut trailers = resp.trailers.clone().unwrap_or_default();
    trailers.insert("content-digest", &encode_digest_map(&values))?;
    resp.trailers = Some(trailers);
    Ok(())
}

fn expected_digest_for_response(
    response: &Response,
    header_name: &str,
    algorithm: DigestAlgorithm,
) -> Result<Vec<u8>, HttpError> {
    let header_value = response
        .headers
        .get(header_name)
        .map(|s| s.to_string())
        .or_else(|| {
            response
                .trailers
                .as_ref()
                .and_then(|t| t.get(header_name).map(|s| s.to_string()))
        })
        .ok_or_else(|| {
            HttpError::new(
                HttpErrorKind::ProtocolError,
                format!("Missing {} field", header_name),
            )
        })?;

    let parsed = parse_digest_map(&header_value)?;
    parsed.get(&algorithm).cloned().ok_or_else(|| {
        HttpError::new(
            HttpErrorKind::ProtocolError,
            format!(
                "Digest field does not contain requested algorithm {}",
                algorithm.token()
            ),
        )
    })
}

fn encode_digest_map(values: &HashMap<DigestAlgorithm, Vec<u8>>) -> String {
    let mut entries = HashMap::new();
    for (alg, bytes) in values {
        entries.insert(
            alg.token().to_string(),
            (
                StructuredValue::Item(StructuredItem::ByteSequence(bytes.clone())),
                StructuredParameters::new(),
            ),
        );
    }
    StructuredFields::encode_dictionary(&StructuredDictionary { entries })
}

fn parse_digest_map(raw: &str) -> Result<HashMap<DigestAlgorithm, Vec<u8>>, HttpError> {
    let dict = StructuredFields::parse_dictionary(raw)?;
    let mut map = HashMap::new();

    for (key, (value, _params)) in dict.entries {
        let alg = match DigestAlgorithm::from_token(&key) {
            Ok(a) => a,
            Err(_) => {
                continue;
            }
        };

        let bytes = match value {
            StructuredValue::Item(StructuredItem::ByteSequence(b)) => b,
            _ => {
                return Err(HttpError::new(
                    HttpErrorKind::ParseError,
                    format!("Digest value for {} must be a byte sequence", key),
                ));
            }
        };

        map.insert(alg, bytes);
    }

    if map.is_empty() {
        return Err(HttpError::new(
            HttpErrorKind::ParseError,
            "Digest field did not contain any supported algorithm",
        ));
    }

    Ok(map)
}

enum DigestState {
    Sha256(Sha256),
    Sha512(Sha512),
    Sha512_256(Sha512_256),
    Blake3(blake3::Hasher),
}

impl DigestState {
    fn new(alg: DigestAlgorithm) -> Self {
        match alg {
            DigestAlgorithm::Sha256 => Self::Sha256(Sha256::new()),
            DigestAlgorithm::Sha512 => Self::Sha512(Sha512::new()),
            DigestAlgorithm::Sha512_256 => Self::Sha512_256(Sha512_256::new()),
            DigestAlgorithm::Blake3 => Self::Blake3(blake3::Hasher::new()),
        }
    }

    fn update(&mut self, chunk: &[u8]) {
        match self {
            Self::Sha256(h) => h.update(chunk),
            Self::Sha512(h) => h.update(chunk),
            Self::Sha512_256(h) => h.update(chunk),
            Self::Blake3(h) => {
                h.update(chunk);
            }
        }
    }

    fn finalize(self) -> Vec<u8> {
        match self {
            Self::Sha256(h) => h.finalize().to_vec(),
            Self::Sha512(h) => h.finalize().to_vec(),
            Self::Sha512_256(h) => h.finalize().to_vec(),
            Self::Blake3(h) => h.finalize().as_bytes().to_vec(),
        }
    }
}

struct StreamingDigestVerifier {
    algorithm: DigestAlgorithm,
    state: Option<DigestState>,
    expected: Vec<u8>,
    finalized: bool,
    emitted_terminal_error: bool,
}

impl StreamingDigestVerifier {
    fn new(algorithm: DigestAlgorithm, expected: Vec<u8>) -> Self {
        Self {
            algorithm,
            state: Some(DigestState::new(algorithm)),
            expected,
            finalized: false,
            emitted_terminal_error: false,
        }
    }

    fn update(&mut self, chunk: &[u8]) {
        if let Some(state) = self.state.as_mut() {
            state.update(chunk);
        }
    }

    fn finish_and_check(&mut self) -> Result<(), HttpError> {
        if self.finalized {
            return Ok(());
        }

        let state = self.state.take().ok_or_else(|| {
            HttpError::new(
                HttpErrorKind::ProtocolError,
                "Streaming digest verifier internal state missing",
            )
        })?;

        let actual = state.finalize();
        self.finalized = true;

        if actual == self.expected {
            Ok(())
        } else {
            Err(HttpError::new(
                HttpErrorKind::SecurityViolation,
                format!(
                    "Content-Digest verification failed for {}",
                    self.algorithm.token()
                ),
            ))
        }
    }
}

fn make_verifying_stream(
    source: BodyStreamCallback,
    verifier: Arc<Mutex<StreamingDigestVerifier>>,
) -> BodyStreamCallback {
    Arc::new(move || match source() {
        Some(Ok(chunk)) => {
            if let Ok(mut guard) = verifier.lock() {
                guard.update(&chunk);
            }
            Some(Ok(chunk))
        }
        Some(Err(e)) => Some(Err(e)),
        None => {
            let mut guard = match verifier.lock() {
                Ok(g) => g,
                Err(_) => {
                    return Some(Err(HttpError::new(
                        HttpErrorKind::ProtocolError,
                        "Streaming digest verifier lock poisoned",
                    )));
                }
            };

            if guard.finalized {
                return None;
            }

            match guard.finish_and_check() {
                Ok(()) => None,
                Err(err) => {
                    if guard.emitted_terminal_error {
                        None
                    } else {
                        guard.emitted_terminal_error = true;
                        Some(Err(err))
                    }
                }
            }
        }
    })
}

pub fn content_digest_base64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
