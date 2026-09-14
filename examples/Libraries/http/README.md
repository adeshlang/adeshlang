# AdeshLang HTTP Library Examples

Production-grade HTTP library for AdeshLang with HTTP/1.1, HTTP/2, HTTP/3 support.

## Quick Start

```sh
# Start a server (stays alive until Ctrl+C)
cargo run --bin adeshlang -- run examples/Libraries/http/index.adesh

# Run all client demos (requires internet)
cargo run --bin adeshlang -- run examples/Libraries/http/client.adesh

# Advanced features demo
cargo run --bin adeshlang -- run examples/Libraries/http/advanced.adesh
```

---

## Examples

### `index.adesh` — Quick Start Server
Minimal Express-style HTTP server with routing and a listen callback.

```adesh
import HTTP;

let router = HTTP.Router();

router.get("/", fn(req, res) {
    return res.json("{\"message\":\"Hello!\"}");
});

let server = HTTP.Server("127.0.0.1:3000", router);
server.listen(fn() {
    print("Server running on http://127.0.0.1:3000");
});
```

### `server.adesh` — Full HTTP Server
Complete REST API server with GET, POST, PUT, DELETE routes, JSON responses, and a pretty startup banner.

Routes demonstrated:
| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | Service info |
| GET | `/health` | Health check |
| GET | `/api/users` | List all users |
| GET | `/api/user?id=1` | Get user by ID |
| POST | `/api/users` | Create user |
| PUT | `/api/users` | Update user |
| DELETE | `/api/users` | Delete user |
| GET | `/about` | HTML text response |

### `client.adesh` — HTTP Client
Covers all HTTP methods using `httpbin.org` as a public test API:
- `HTTP.get()` / `HTTP.post()` / `HTTP.put()` / `HTTP.delete()` / `HTTP.patch()`
- `HTTP.head()` / `HTTP.options()` / `HTTP.query()`
- `HTTP.Client()` for persistent client instances
- Status code helpers: `HTTP.Status.isSuccess()`, `HTTP.Status.isRetryable()`, etc.
- Method helpers: `HTTP.Method.isSafe()`, `HTTP.Method.isIdempotent()`, etc.

### `advanced.adesh` — Advanced Features
- **Explain mode** — diagnose a URL without sending a request
- **Status constants** — `HTTP.Status.OK`, `HTTP.Status.NotFound`, etc.
- **Method constants** — `HTTP.Method.GET`, `HTTP.Method.POST`, etc.
- **HttpVersion constants** — `HTTP.HttpVersion.HTTP_1_1`, etc.
- **HttpError constants** — `HTTP.HttpError.Timeout`, etc.
- **Reverse Proxy** — `HTTP.ReverseProxy([...upstreams])`

---

## API Reference

### HTTP Client Functions

```adesh
HTTP.get(url)                        // GET request
HTTP.post(url, body)                 // POST with body string
HTTP.put(url, body)                  // PUT with body string
HTTP.patch(url, body)                // PATCH with body string
HTTP.delete(url)                     // DELETE request
HTTP.head(url)                       // HEAD request (headers only)
HTTP.options(url)                    // OPTIONS request
HTTP.query(url, body)                // QUERY request (RFC 9110)
HTTP.custom(method, url, body)       // Custom HTTP method
HTTP.explain(url)                    // Diagnose URL without sending
```

### Response Object

All request functions return a response object:

```adesh
res.status       // int — HTTP status code (e.g. 200, 404)
res.statusText   // string — reason phrase (e.g. "OK", "Not Found")
res.ok           // bool — true if status 200-299
res.text()       // fn -> string — body as text
res.json()       // fn -> object — body parsed as JSON
res.bytes()      // fn -> array — body as byte array
res.headers      // object — response headers
```

### HTTP Server

```adesh
let router = HTTP.Router();

// Route registration — method can be: get, post, put, delete, patch, head, options
router.get(path, fn(req, res) { return res.send("text"); });
router.post(path, fn(req, res) { return res.json("{}"); });

// Server creation and startup
let server = HTTP.Server("host:port", router);
server.listen(fn() { print("Listening!"); });  // Blocks until Ctrl+C
```

### Request Object (inside route callbacks)

```adesh
req.method       // string — "GET", "POST", etc.
req.path         // string — URL path (e.g. "/api/users")
req.url          // string — full URL
req.requestId    // string — unique request ID
req.query("key") // fn(string) -> string — query parameter
req.param("key") // fn(string) -> string — route parameter
```

### Response Builder (inside route callbacks)

```adesh
res.send("text")     // Send plain text response, returns response object
res.json("{}}")      // Send JSON response, returns response object
res.status(code)     // Set status code (call before send/json)
res.setHeader(k, v)  // Set a response header
```

### Persistent HTTP Client

```adesh
let client = HTTP.Client();
client.get(url)
client.post(url, body)
client.put(url, body)
client.delete(url)
client.patch(url, body)
client.head(url)
client.options(url)
```

### Status & Method Utilities

```adesh
HTTP.Status.OK                    // 200
HTTP.Status.isSuccess(code)       // code 200-299
HTTP.Status.isClientError(code)   // code 400-499
HTTP.Status.isServerError(code)   // code 500-599
HTTP.Status.isRetryable(code)     // 408, 425, 429, 500, 502, 503, 504

HTTP.Method.GET                   // "GET"
HTTP.Method.isSafe(method)        // GET, HEAD, OPTIONS, TRACE
HTTP.Method.isIdempotent(method)  // GET, HEAD, PUT, DELETE, OPTIONS, TRACE
HTTP.Method.isCacheable(method)   // GET, HEAD, POST
```
