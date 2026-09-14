#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSentEvent {
    pub event: Option<String>,
    pub data: String,
    pub id: Option<String>,
    pub retry_ms: Option<u64>,
}

impl ServerSentEvent {
    pub fn new(data: impl Into<String>) -> Self {
        Self {
            event: None,
            data: data.into(),
            id: None,
            retry_ms: None,
        }
    }

    pub fn encode(&self) -> String {
        let mut out = String::new();
        if let Some(ref ev) = self.event {
            out.push_str(&format!("event: {}\n", ev));
        }
        if let Some(ref id) = self.id {
            out.push_str(&format!("id: {}\n", id));
        }
        if let Some(ret) = self.retry_ms {
            out.push_str(&format!("retry: {}\n", ret));
        }
        for line in self.data.lines() {
            out.push_str(&format!("data: {}\n", line));
        }
        out.push('\n');
        out
    }
}
