use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HttpStatus(pub u16);

impl HttpStatus {
    // 1xx Informational
    pub const CONTINUE: HttpStatus = HttpStatus(100);
    pub const SWITCHING_PROTOCOLS: HttpStatus = HttpStatus(101);
    pub const PROCESSING: HttpStatus = HttpStatus(102);
    pub const EARLY_HINTS: HttpStatus = HttpStatus(103);

    // 2xx Success
    pub const OK: HttpStatus = HttpStatus(200);
    pub const CREATED: HttpStatus = HttpStatus(201);
    pub const ACCEPTED: HttpStatus = HttpStatus(202);
    pub const NON_AUTHORITATIVE_INFORMATION: HttpStatus = HttpStatus(203);
    pub const NO_CONTENT: HttpStatus = HttpStatus(204);
    pub const RESET_CONTENT: HttpStatus = HttpStatus(205);
    pub const PARTIAL_CONTENT: HttpStatus = HttpStatus(206);
    pub const MULTI_STATUS: HttpStatus = HttpStatus(207);
    pub const ALREADY_REPORTED: HttpStatus = HttpStatus(208);
    pub const IM_USED: HttpStatus = HttpStatus(226);

    // 3xx Redirection
    pub const MULTIPLE_CHOICES: HttpStatus = HttpStatus(300);
    pub const MOVED_PERMANENTLY: HttpStatus = HttpStatus(301);
    pub const FOUND: HttpStatus = HttpStatus(302);
    pub const SEE_OTHER: HttpStatus = HttpStatus(303);
    pub const NOT_MODIFIED: HttpStatus = HttpStatus(304);
    pub const USE_PROXY: HttpStatus = HttpStatus(305);
    pub const TEMPORARY_REDIRECT: HttpStatus = HttpStatus(307);
    pub const PERMANENT_REDIRECT: HttpStatus = HttpStatus(308);

    // 4xx Client Error
    pub const BAD_REQUEST: HttpStatus = HttpStatus(400);
    pub const UNAUTHORIZED: HttpStatus = HttpStatus(401);
    pub const PAYMENT_REQUIRED: HttpStatus = HttpStatus(402);
    pub const FORBIDDEN: HttpStatus = HttpStatus(403);
    pub const NOT_FOUND: HttpStatus = HttpStatus(404);
    pub const METHOD_NOT_ALLOWED: HttpStatus = HttpStatus(405);
    pub const NOT_ACCEPTABLE: HttpStatus = HttpStatus(406);
    pub const PROXY_AUTHENTICATION_REQUIRED: HttpStatus = HttpStatus(407);
    pub const REQUEST_TIMEOUT: HttpStatus = HttpStatus(408);
    pub const CONFLICT: HttpStatus = HttpStatus(409);
    pub const GONE: HttpStatus = HttpStatus(410);
    pub const LENGTH_REQUIRED: HttpStatus = HttpStatus(411);
    pub const PRECONDITION_FAILED: HttpStatus = HttpStatus(412);
    pub const CONTENT_TOO_LARGE: HttpStatus = HttpStatus(413);
    pub const URI_TOO_LONG: HttpStatus = HttpStatus(414);
    pub const UNSUPPORTED_MEDIA_TYPE: HttpStatus = HttpStatus(415);
    pub const RANGE_NOT_SATISFIABLE: HttpStatus = HttpStatus(416);
    pub const EXPECTATION_FAILED: HttpStatus = HttpStatus(417);
    pub const IM_A_TEAPOT: HttpStatus = HttpStatus(418);
    pub const MISDIRECTED_REQUEST: HttpStatus = HttpStatus(421);
    pub const UNPROCESSABLE_CONTENT: HttpStatus = HttpStatus(422);
    pub const LOCKED: HttpStatus = HttpStatus(423);
    pub const FAILED_DEPENDENCY: HttpStatus = HttpStatus(424);
    pub const TOO_EARLY: HttpStatus = HttpStatus(425);
    pub const UPGRADE_REQUIRED: HttpStatus = HttpStatus(426);
    pub const PRECONDITION_REQUIRED: HttpStatus = HttpStatus(428);
    pub const TOO_MANY_REQUESTS: HttpStatus = HttpStatus(429);
    pub const REQUEST_HEADER_FIELDS_TOO_LARGE: HttpStatus = HttpStatus(431);
    pub const UNAVAILABLE_FOR_LEGAL_REASONS: HttpStatus = HttpStatus(451);

    // 5xx Server Error
    pub const INTERNAL_SERVER_ERROR: HttpStatus = HttpStatus(500);
    pub const NOT_IMPLEMENTED: HttpStatus = HttpStatus(501);
    pub const BAD_GATEWAY: HttpStatus = HttpStatus(502);
    pub const SERVICE_UNAVAILABLE: HttpStatus = HttpStatus(503);
    pub const GATEWAY_TIMEOUT: HttpStatus = HttpStatus(504);
    pub const HTTP_VERSION_NOT_SUPPORTED: HttpStatus = HttpStatus(505);
    pub const VARIANT_ALSO_NEGOTIATES: HttpStatus = HttpStatus(506);
    pub const INSUFFICIENT_STORAGE: HttpStatus = HttpStatus(507);
    pub const LOOP_DETECTED: HttpStatus = HttpStatus(508);
    pub const NOT_EXTENDED: HttpStatus = HttpStatus(510);
    pub const NETWORK_AUTHENTICATION_REQUIRED: HttpStatus = HttpStatus(511);

    pub fn code(&self) -> u16 {
        self.0
    }

    pub fn is_informational(&self) -> bool {
        (100..200).contains(&self.0)
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.0)
    }

    pub fn is_redirection(&self) -> bool {
        (300..400).contains(&self.0)
    }

    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.0)
    }

    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.0)
    }

    pub fn is_error(&self) -> bool {
        self.is_client_error() || self.is_server_error()
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self.0, 408 | 425 | 429 | 500 | 502 | 503 | 504)
    }

    pub fn is_cacheable(&self) -> bool {
        matches!(
            self.0,
            200 | 203 | 204 | 206 | 300 | 301 | 308 | 404 | 405 | 410 | 414 | 501
        )
    }

    pub fn reason_phrase(&self) -> &'static str {
        match self.0 {
            100 => "Continue",
            101 => "Switching Protocols",
            102 => "Processing",
            103 => "Early Hints",
            200 => "OK",
            201 => "Created",
            202 => "Accepted",
            203 => "Non-Authoritative Information",
            204 => "No Content",
            205 => "Reset Content",
            206 => "Partial Content",
            207 => "Multi-Status",
            208 => "Already Reported",
            226 => "IM Used",
            300 => "Multiple Choices",
            301 => "Moved Permanently",
            302 => "Found",
            303 => "See Other",
            304 => "Not Modified",
            305 => "Use Proxy",
            307 => "Temporary Redirect",
            308 => "Permanent Redirect",
            400 => "Bad Request",
            401 => "Unauthorized",
            402 => "Payment Required",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            406 => "Not Acceptable",
            407 => "Proxy Authentication Required",
            408 => "Request Timeout",
            409 => "Conflict",
            410 => "Gone",
            411 => "Length Required",
            412 => "Precondition Failed",
            413 => "Content Too Large",
            414 => "URI Too Long",
            415 => "Unsupported Media Type",
            416 => "Range Not Satisfiable",
            417 => "Expectation Failed",
            418 => "I'm a teapot",
            421 => "Misdirected Request",
            422 => "Unprocessable Content",
            423 => "Locked",
            424 => "Failed Dependency",
            425 => "Too Early",
            426 => "Upgrade Required",
            428 => "Precondition Required",
            429 => "Too Many Requests",
            431 => "Request Header Fields Too Large",
            451 => "Unavailable For Legal Reasons",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            504 => "Gateway Timeout",
            505 => "HTTP Version Not Supported",
            506 => "Variant Also Negotiates",
            507 => "Insufficient Storage",
            508 => "Loop Detected",
            510 => "Not Extended",
            511 => "Network Authentication Required",
            _ => "Unknown",
        }
    }
}

impl fmt::Display for HttpStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.0, self.reason_phrase())
    }
}

impl From<u16> for HttpStatus {
    fn from(c: u16) -> Self {
        HttpStatus(c)
    }
}
