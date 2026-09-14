# AdeshLang `URL` Standard Library

A production-grade, standards-compliant (RFC 3986, RFC 3987, WHATWG URL, IDNA 2008), memory-safe, ownership-aware URL processing engine for **AdeshLang**.

---

## Table of Contents
1. [Importing the Library](#importing-the-library)
2. [Module API Reference (`URL`)](#module-api-reference-url)
3. [`URL` Object API](#url-object-api)
4. [`URLBuilder` API](#urlbuilder-api)
5. [`URLPattern` API](#urlpattern-api)
6. [`URLTemplate` API](#urltemplate-api)
7. [`URLSecurityPolicy` API](#urlsecuritypolicy-api)
8. [`URLView` Zero-Copy Parser API](#urlview-zero-copy-parser-api)
9. [`DataURL` & Special Schemes](#dataurl--special-schemes)
10. [Complete Executable Examples Suite](#complete-executable-examples-suite)

---

## Importing the Library

```adesh
// Standard import
import URL;

// Explicit namespace import
import std:URL;

// Named import
import { parse, builder, pattern, SecurityPolicy } from "std:URL";
```

---

## Module API Reference (`URL`)

### Parsing & Static Construction

#### `URL.parse(urlString: String): URL`
Parses an absolute URL string according to RFC 3986 / WHATWG standards.
```adesh
let url: URL = URL.parse("https://user:pass@example.com:8080/path?q=1#top");
```

#### `URL.parseRelative(relativeUrlString: String, baseUrl: URL): URL`
Parses a relative URL string against an existing `URL` base object.
```adesh
let base: URL = URL.parse("https://example.com/docs/");
let resolved: URL = URL.parseRelative("../api/v1", base);
```

#### `URL.builder(): URLBuilder`
Instantiates a new fluent `URLBuilder`.
```adesh
let builder: URLBuilder = URL.builder();
```

#### `URL.resolve(baseUrl: URL, relativeUrlString: String): URL`
Resolves a relative reference path against a base URL object.
```adesh
let base: URL = URL.parse("https://example.com/a/b/c");
let target: URL = URL.resolve(base, "../../d");
```

#### `URL.join(baseUrl: URL, relativePath: String): URL`
Appends a path segment to a base URL cleanly without dot-segment resolution errors.
```adesh
let joined: URL = URL.join(base, "v2/items");
```

#### `URL.isValid(urlString: String): Bool`
Returns `true` if `urlString` can be successfully parsed into a valid URL.
```adesh
let valid: Bool = URL.isValid("https://example.com"); // true
```

#### `URL.isAbsolute(urlString: String): Bool`
Returns `true` if `urlString` contains an explicit scheme identifier.
```adesh
let abs: Bool = URL.isAbsolute("https://example.com"); // true
```

#### `URL.isRelative(urlString: String): Bool`
Returns `true` if `urlString` is a relative path or reference.
```adesh
let rel: Bool = URL.isRelative("/api/v1/users"); // true
```

---

### Encoding & Decoding Helpers

- `URL.percentEncode(text: String): String`: Percent-encodes reserved characters per RFC 3986.
- `URL.percentDecode(encodedText: String): String`: Percent-decodes percent-encoded bytes into UTF-8.
- `URL.formEncode(text: String): String`: Form URL-encodes string (`application/x-www-form-urlencoded`, spaces -> `+`).
- `URL.formDecode(encodedText: String): String`: Form URL-decodes string (`+` -> spaces).

---

### Special Schemes & Utility Methods

- `URL.fromFilePath(pathString: String): URL`: Converts a local file system path into a `file://` URL.
- `URL.toFilePath(urlObject: URL): String`: Converts a `file://` URL into a local file system path string.
- `URL.parseData(dataUrlString: String): DataURL`: Parses a `data:` URI into a `DataURL` object.
- `URL.toDataUrl(mediaType: String, payload: String, isBase64: Bool): String`: Encodes a payload into a `data:` URI string.
- `URL.parseView(urlString: String): URLView`: Zero-copy slice-based borrowed parser.
- `URL.domainToAscii(domainString: String): String`: Converts Unicode domain name to IDNA ASCII Punycode (`xn--...`).
- `URL.domainToUnicode(punycodeString: String): String`: Converts IDNA ASCII Punycode domain to Unicode representation.
- `URL.diff(urlA: URL, urlB: URL): Vec<String>`: Compares two URL objects and returns a array of component differences.
- `URL.securityReport(urlObject: URL): Vec<String>`: Scans URL for embedded credentials, suspicious Punycode homographs, and SSRF private IP targets.

---

## `URL` Object API

### Properties
- `url.scheme: String`: Protocol scheme (e.g. `"https"`, `"http"`, `"file"`).
- `url.username: Option<String>`: Userinfo username component, or `null`.
- `url.password: Option<String>`: Userinfo password component, or `null`.
- `url.host: Option<String>`: Hostname or IP address (e.g. `"example.com"`, `"192.168.1.1"`).
- `url.hostname: Option<String>`: Hostname without port number.
- `url.port: Option<Int>`: Explicit port number, or `null` if default port is omitted.
- `url.path: String`: Normalized dot-segment path (e.g. `"/api/v1"`).
- `url.query: String`: Percent-encoded query string without leading `'?'`.
- `url.fragment: Option<String>`: Fragment hash identifier without leading `'#'`.
- `url.href: String`: Full canonical URL string representation.
- `url.origin: String`: Web origin (scheme + host + port).
- `url.authority: String`: Full authority block (`user:pass@host:port`).

### Instance Methods
- `url.redacted(): String`: Returns href with password masked (`***`).
- `url.safeString(): String`: Alias for `url.redacted()`.
- `url.canonicalize(): URL`: Returns canonicalized URL object.
- `url.stripCredentials(): URL`: Returns new URL with username and password removed.
- `url.stripFragment(): URL`: Returns new URL with fragment hash removed.
- `url.stripTrackingParameters(): URL`: Removes known marketing tracking parameters (`utm_source`, `fbclid`, `gclid`, `ref_`).
- `url.isHttp(): Bool`: Returns `true` if scheme is `"http"`.
- `url.isHttps(): Bool`: Returns `true` if scheme is `"https"`.
- `url.isWebSocket(): Bool`: Returns `true` if scheme is `"ws"` or `"wss"`.
- `url.isSecure(): Bool`: Returns `true` if scheme is `"https"` or `"wss"`.
- `url.sameOrigin(otherUrl: URL): Bool`: Returns `true` if both URLs share identical origin.
- `url.toWebSocketUrl(): URL`: Converts HTTP/HTTPS URL to WS/WSS URL.
- `url.toHttpUrl(): URL`: Converts WS/WSS URL to HTTP/HTTPS URL.
- `url.withScheme(newScheme: String): URL`: Returns updated URL copy with new scheme.
- `url.withHost(newHost: String): URL`: Returns updated URL copy with new host.
- `url.withPort(newPort: Int): URL`: Returns updated URL copy with new port.
- `url.withPath(newPath: String): URL`: Returns updated URL copy with new path.
- `url.withQueryParam(key: String, value: Any): URL`: Returns updated URL with added query parameter.
- `url.removeQueryParam(key: String): URL`: Returns updated URL with query parameter removed.
- `url.appendPathSegment(segment: String): URL`: Returns updated URL with appended path segment.
- `url.edit(): URLBuilder`: Returns a `URLBuilder` pre-populated with this URL's components.

---

## `URLBuilder` API

Fluent builder for assembling URLs securely:

```adesh
let url: URL = URL.builder()
    .scheme("https")
    .host("api.service.com")
    .port(443)
    .path("/v1")
    .appendPathSegment("resource")
    .queryParam("limit", 50)
    .queryParam("sort", "desc")
    .fragment("section1")
    .build();
```

---

## `URLPattern` API

Route pattern matcher supporting named parameters (`:param`):

```adesh
let pattern: URLPattern = URL.pattern("https://api.example.com/v1/users/:userId/orders/:orderId");
let match: Option<URLPatternMatch> = pattern.match(targetUrl);

if (match != null) {
    print("User ID:", match.params.userId);
    print("Order ID:", match.params.orderId);
}
```

---

## `URLTemplate` API

URI Template expansion engine:

```adesh
let template: URLTemplate = URL.template("https://api.example.com/v1/projects/{projectId}/issues/{issueId}");
let url: URL = template.expand({
    "projectId": "adesh-lang",
    "issueId": "42"
});
```

---

## `URLSecurityPolicy` API

Configurable SSRF target protection, domain restriction, and redirect safety validation:

```adesh
let policy: URLSecurityPolicy = URL.SecurityPolicy()
    .allowScheme("https")
    .allowDomain("*.example.com") // Wildcard domain matching
    .denyPrivateNetworks()       // Blocks 127.0.0.1, 10.0.0.0/8, 192.168.0.0/16, 169.254.169.254
    .denyCredentials();

try {
    policy.validate(targetUrl);
    print("Target is safe!");
} catch(err) {
    print("Security policy blocked URL:", err);
}
```

---

## `URLView` Zero-Copy Parser API

High-performance slice-based zero-copy parser for non-allocating string inspection:

```adesh
let buffer: String = "https://stream.example.com:8443/feed";
let view: URLView = URL.parseView(buffer);

print("View Scheme:", view.scheme());
print("View Host:", view.host());

let ownedUrl: URL = view.toOwned();
```

---

## `DataURL` & Special Schemes

```adesh
// Data URL creation and decoding
let dataUrlStr: String = URL.toDataUrl("text/plain", "Hello AdeshLang!", true);
let dataObj: DataURL = URL.parseData(dataUrlStr);

print("Media Type:", dataObj.mediaType);
print("Is Base64:", dataObj.isBase64);
print("Decoded String:", dataObj.decodeString());

// File URL conversion
let fileUrl: URL = URL.fromFilePath("/var/log/app.log");
let localPath: String = URL.toFilePath(fileUrl);
```

---

## Complete Executable Examples Suite

The `examples/libraries/url/` folder contains **17 complete, runnable example scripts** with explicit type annotations (`: URL`, `: String`, etc.):

| Script Name | Description | Command |
| :--- | :--- | :--- |
| [`01_basic_parsing.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/01_basic_parsing.adesh) | Basic URL parsing, component accessors, and credential redaction | `cargo run --bin adeshlang -- run examples/libraries/url/01_basic_parsing.adesh` |
| [`02_url_components.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/02_url_components.adesh) | Username, password, authority, origin, and protocol properties | `cargo run --bin adeshlang -- run examples/libraries/url/02_url_components.adesh` |
| [`03_url_builder.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/03_url_builder.adesh) | Safe URL construction via `URLBuilder` fluent interface | `cargo run --bin adeshlang -- run examples/libraries/url/03_url_builder.adesh` |
| [`04_relative_resolution.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/04_relative_resolution.adesh) | Relative reference resolution (`../`, `/search`) against base URL | `cargo run --bin adeshlang -- run examples/libraries/url/04_relative_resolution.adesh` |
| [`05_query_manipulation.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/05_query_manipulation.adesh) | Immutable query parameter updates (`withQueryParam`, `removeQueryParam`) | `cargo run --bin adeshlang -- run examples/libraries/url/05_query_manipulation.adesh` |
| [`06_ipv4_ipv6_handling.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/06_ipv4_ipv6_handling.adesh) | Dotted quad IPv4 and compressed IPv6 literal (`[::1]`) handling | `cargo run --bin adeshlang -- run examples/libraries/url/06_ipv4_ipv6_handling.adesh` |
| [`07_idna_punycode_unicode.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/07_idna_punycode_unicode.adesh) | IDNA Punycode encoding/decoding (`domainToAscii`, `domainToUnicode`) | `cargo run --bin adeshlang -- run examples/libraries/url/07_idna_punycode_unicode.adesh` |
| [`08_pattern_matching.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/08_pattern_matching.adesh) | `URLPattern` route parameter extraction (`/users/:userId/posts/:postId`) | `cargo run --bin adeshlang -- run examples/libraries/url/08_pattern_matching.adesh` |
| [`09_template_expansion.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/09_template_expansion.adesh) | `URLTemplate` expansion engine for URI templates | `cargo run --bin adeshlang -- run examples/libraries/url/09_template_expansion.adesh` |
| [`10_security_ssrf_policies.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/10_security_ssrf_policies.adesh) | `URLSecurityPolicy` SSRF prevention and private network blocking | `cargo run --bin adeshlang -- run examples/libraries/url/10_security_ssrf_policies.adesh` |
| [`11_redirect_validation.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/11_redirect_validation.adesh) | URL diffing (`URL.diff`) and redirect safety verification | `cargo run --bin adeshlang -- run examples/libraries/url/11_redirect_validation.adesh` |
| [`12_file_data_special_urls.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/12_file_data_special_urls.adesh) | `file://` conversion and Base64 `data:` URI payload decoding | `cargo run --bin adeshlang -- run examples/libraries/url/12_file_data_special_urls.adesh` |
| [`13_zero_copy_url_view.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/13_zero_copy_url_view.adesh) | High-performance zero-copy slice parsing with `URLView` | `cargo run --bin adeshlang -- run examples/libraries/url/13_zero_copy_url_view.adesh` |
| [`14_tracking_cleanup.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/14_tracking_cleanup.adesh) | Automated privacy tracking parameter cleanup (`stripTrackingParameters`) | `cargo run --bin adeshlang -- run examples/libraries/url/14_tracking_cleanup.adesh` |
| [`15_real_world_rest_api.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/15_real_world_rest_api.adesh) | Production REST API client endpoint construction | `cargo run --bin adeshlang -- run examples/libraries/url/15_real_world_rest_api.adesh` |
| [`16_real_world_webhook_security.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/16_real_world_webhook_security.adesh) | Webhook URL safety validation against cloud metadata SSRF | `cargo run --bin adeshlang -- run examples/libraries/url/16_real_world_webhook_security.adesh` |
| [`17_edge_cases_and_matrix.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/url/17_edge_cases_and_matrix.adesh) | Edge cases and multi-scheme parsing matrix | `cargo run --bin adeshlang -- run examples/libraries/url/17_edge_cases_and_matrix.adesh` |
