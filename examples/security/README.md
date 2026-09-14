# Security Examples

Examples demonstrating security best practices in AdeshLang.

## Files

| File | Description |
|------|-------------|
| `ssrf_prevention.adesh` | Preventing Server-Side Request Forgery attacks with URL validation, domain allowlists, and internal IP blocking. |

## SSRF Prevention

### What is SSRF?
Server-Side Request Forgery tricks an application into making unintended requests to:
- Internal networks (10.x, 172.x, 192.168.x)
- Cloud metadata endpoints (169.254.169.254)
- Localhost services

### Protection Strategies

1. **URL Validation**: Parse and validate before any network call
2. **Protocol Allowlist**: Only allow http/https
3. **Host Blocking**: Block private IP ranges and localhost
4. **Domain Allowlist**: Maintain explicit list of allowed domains

### Example

```adesh
fn safeRequest(url) {
    // 1. Validate protocol
    if (!isProtocolAllowed(parsed.protocol)) {
        return error("Bad protocol");
    }
    
    // 2. Block internal hosts
    if (isInternalHost(parsed.host)) {
        return error("Internal host blocked");
    }
    
    // 3. Check domain allowlist
    if (!isDomainAllowed(parsed.host)) {
        return error("Domain not allowed");
    }
    
    // 4. Make request
    return httpGet(url);
}
```

## Running Examples

```bash
adesh run examples/security/ssrf_prevention.adesh
```

## Related Documentation

- [Safe I/O Examples](../fs/README.md)
- [Examples Index](../../docs/examples.md)
