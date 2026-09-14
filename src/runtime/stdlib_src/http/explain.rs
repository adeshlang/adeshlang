use super::version::HttpVersion;
use std::fmt;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct HttpExplanation {
    pub url: String,
    pub protocol: HttpVersion,
    pub tls_version: Option<String>,
    pub dns_resolution_time: Duration,
    pub connect_time: Duration,
    pub ttfb: Duration,
    pub connection_reused: bool,
    pub cache_status: String,
    pub compression: Option<String>,
    pub bytes_sent: usize,
    pub bytes_received: usize,
}

impl fmt::Display for HttpExplanation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== AdeshLang Explainable HTTP Diagnostic ===")?;
        writeln!(f, "URL:                {}", self.url)?;
        writeln!(f, "Protocol:           {}", self.protocol)?;
        if let Some(ref tls) = self.tls_version {
            writeln!(f, "TLS:                {}", tls)?;
        }
        writeln!(
            f,
            "Connection Reused:  {}",
            if self.connection_reused { "yes" } else { "no" }
        )?;
        writeln!(f, "Cache:              {}", self.cache_status)?;
        if let Some(ref comp) = self.compression {
            writeln!(f, "Compression:        {}", comp)?;
        }
        writeln!(f, "DNS Time:           {:?}", self.dns_resolution_time)?;
        writeln!(f, "Connect Time:       {:?}", self.connect_time)?;
        writeln!(f, "Time to First Byte: {:?}", self.ttfb)?;
        writeln!(f, "Bytes Sent:         {}", self.bytes_sent)?;
        writeln!(f, "Bytes Received:     {}", self.bytes_received)?;
        Ok(())
    }
}
