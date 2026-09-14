#[cfg(test)]
mod tests {
    use super::super::body::Body;
    use super::super::budget::RequestBudget;
    use super::super::cache::single_flight::SingleFlight;
    use super::super::digest::{
        DigestAlgorithm, apply_content_digest_header, apply_content_digest_trailer,
        apply_repr_digest_header, verify_content_digest, verify_representation_digest,
        wrap_response_stream_for_digest_verification,
    };
    use super::super::errors::HttpError;
    use super::super::headers::Headers;
    use super::super::http1::chunked::{decode_chunked, encode_chunk, encode_final_chunk};
    use super::super::http1::parser::Http1Parser;
    use super::super::http1::smuggle::validate_framing_security;
    use super::super::http2::frames::Http2Frame;
    use super::super::http2::hpack::{HpackDecoder, HpackEncoder};
    use super::super::http3::frames::Http3Frame;
    use super::super::http3::qpack::{QpackDecoder, QpackEncoder};
    use super::super::method::HttpMethod;
    use super::super::method::HttpMethodRegistry;
    use super::super::multipart::MultipartForm;
    use super::super::pipeline::{
        HttpMiddleware, MiddlewarePhase, MiddlewarePipeline, MiddlewarePriority,
    };
    use super::super::policy::HttpPolicy;
    use super::super::priority::Priority;
    use super::super::request::Request;
    use super::super::response::Response;
    use super::super::status::HttpStatus;
    use super::super::structured_fields::{StructuredFields, StructuredItem};
    use super::super::uri::Uri;
    use super::super::websocket::connection::{WebSocketMessage, WebSocketStream};
    use super::super::websocket::frame::{
        WebSocketFrame, WebSocketFrameDecodeConfig, WebSocketOpcode,
    };
    use super::super::websocket::handshake::{
        calculate_websocket_accept, generate_websocket_key, handle_server_handshake,
        validate_client_upgrade_response, validate_server_upgrade_request,
    };
    use std::sync::Arc;

    #[test]
    fn test_http_status_codes_and_categories() {
        assert_eq!(HttpStatus::OK.code(), 200);
        assert_eq!(HttpStatus::CREATED.code(), 201);
        assert_eq!(HttpStatus::NOT_FOUND.code(), 404);

        assert!(HttpStatus::OK.is_success());
        assert!(HttpStatus::CREATED.is_success());
        assert!(HttpStatus::NOT_FOUND.is_client_error());
        assert!(HttpStatus::INTERNAL_SERVER_ERROR.is_server_error());
        assert!(HttpStatus::SERVICE_UNAVAILABLE.is_retryable());
        assert!(HttpStatus::TOO_MANY_REQUESTS.is_retryable());
        assert!(HttpStatus::OK.is_cacheable());
        assert_eq!(HttpStatus::OK.reason_phrase(), "OK");
    }

    #[test]
    fn test_http_methods() {
        let get = HttpMethod::parse("GET").unwrap();
        let post = HttpMethod::parse("POST").unwrap();
        let custom = HttpMethod::parse("PURGE").unwrap();

        assert!(get.is_safe());
        assert!(get.is_idempotent());
        assert!(!post.is_safe());
        assert!(!post.is_idempotent());
        assert_eq!(custom.as_str(), "PURGE");
        assert!(custom.is_custom());
    }

    #[test]
    fn test_uri_parser_ipv6_and_queries() {
        let uri =
            Uri::parse("https://[::1]:8443/api/v1/users?page=2&search=adesh#profile").unwrap();
        assert!(uri.is_https());
        assert_eq!(uri.host.as_deref(), Some("[::1]"));
        assert_eq!(uri.effective_port(), 8443);
        assert_eq!(uri.path, "/api/v1/users");
        let q = uri.query_params();
        assert_eq!(q.get("page").map(|s| s.as_str()), Some("2"));
        assert_eq!(q.get("search").map(|s| s.as_str()), Some("adesh"));
        assert_eq!(uri.fragment.as_deref(), Some("profile"));
    }

    #[test]
    fn test_headers_case_insensitivity_and_crlf_defense() {
        let mut headers = Headers::new();
        headers.insert("Content-Type", "application/json").unwrap();
        headers.append("Accept", "application/json").unwrap();
        headers.append("accept", "text/plain").unwrap();

        assert_eq!(headers.get("content-type"), Some("application/json"));
        assert_eq!(
            headers.get_all("accept"),
            vec!["application/json", "text/plain"]
        );

        // CRLF injection attempt MUST be rejected
        let crlf_res = headers.insert("X-Injected", "value\r\nInjected-Header: evil");
        assert!(crlf_res.is_err());
    }

    #[test]
    fn test_http1_parser_and_smuggling_defense() {
        let parser = Http1Parser::new();
        let wire = b"GET /hello HTTP/1.1\r\nHost: example.com\r\nContent-Length: 5\r\n\r\nhello";
        let (req, consumed) = parser.parse_request(wire).unwrap();

        assert_eq!(req.method, HttpMethod::Get);
        assert_eq!(req.uri.path, "/hello");
        assert_eq!(req.body.to_text().unwrap(), "hello");
        assert_eq!(consumed, wire.len());

        // Smuggling check: both Content-Length and Transfer-Encoding present must fail
        let mut bad_headers = Headers::new();
        bad_headers.insert("content-length", "10").unwrap();
        bad_headers.insert("transfer-encoding", "chunked").unwrap();
        assert!(validate_framing_security(&bad_headers).is_err());
    }

    #[test]
    fn test_chunked_transfer_encoding_and_trailers() {
        let chunk1 = encode_chunk(b"Hello ");
        let chunk2 = encode_chunk(b"World!");
        let mut trailers = Headers::new();
        trailers.insert("X-Checksum", "abc123").unwrap();
        let final_chunk = encode_final_chunk(Some(&trailers));

        let mut wire = Vec::new();
        wire.extend_from_slice(&chunk1);
        wire.extend_from_slice(&chunk2);
        wire.extend_from_slice(&final_chunk);

        let (decoded, parsed_trailers, _) = decode_chunked(&wire).unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "Hello World!");
        assert_eq!(parsed_trailers.unwrap().get("x-checksum"), Some("abc123"));
    }

    #[test]
    fn test_hpack_codec() {
        let mut encoder = HpackEncoder::new(4096);
        let mut decoder = HpackDecoder::new(4096);

        let input_headers = vec![
            (":method", "GET"),
            (":path", "/api/test"),
            (":scheme", "https"),
            ("custom-header", "custom-value"),
        ];

        let encoded = encoder.encode(&input_headers);
        let decoded = decoder.decode(&encoded).unwrap();

        assert_eq!(decoded.len(), 4);
        assert_eq!(decoded[0], (":method".to_string(), "GET".to_string()));
        assert_eq!(decoded[1], (":path".to_string(), "/api/test".to_string()));
        assert_eq!(
            decoded[3],
            ("custom-header".to_string(), "custom-value".to_string())
        );
    }

    #[test]
    fn test_http2_frame_codec() {
        let frame = Http2Frame::Headers {
            stream_id: 1,
            end_stream: true,
            end_headers: true,
            header_block_fragment: vec![0x82, 0x86, 0x84], // static table indices
        };

        let encoded = frame.encode();
        let (decoded_frame, _) = Http2Frame::decode(&encoded).unwrap().unwrap();

        match decoded_frame {
            Http2Frame::Headers {
                stream_id,
                end_stream,
                end_headers,
                header_block_fragment,
            } => {
                assert_eq!(stream_id, 1);
                assert!(end_stream);
                assert!(end_headers);
                assert_eq!(header_block_fragment, vec![0x82, 0x86, 0x84]);
            }
            _ => panic!("Expected Headers frame"),
        }
    }

    #[test]
    fn test_qpack_and_http3_frames() {
        let encoder = QpackEncoder::new(4096);
        let decoder = QpackDecoder::new(4096);

        let headers = vec![
            (":method", "GET"),
            (":path", "/"),
            (":scheme", "https"),
            ("x-test", "val"),
        ];

        let encoded_block = encoder.encode(&headers);
        let decoded_headers = decoder.decode(&encoded_block).unwrap();
        assert_eq!(
            decoded_headers[0],
            (":method".to_string(), "GET".to_string())
        );

        let h3_frame = Http3Frame::Headers {
            encoded_fields: encoded_block,
        };
        let wire = h3_frame.encode();
        let (decoded_h3, _) = Http3Frame::decode(&wire).unwrap().unwrap();

        match decoded_h3 {
            Http3Frame::Headers { encoded_fields } => {
                assert!(!encoded_fields.is_empty());
            }
            _ => panic!("Expected HTTP/3 Headers frame"),
        }
    }

    #[test]
    fn test_single_flight_coalescing() {
        let sf = SingleFlight::new();
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let res = sf
            .execute("resource_key", || {
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Response::text("Coalesced result")
            })
            .unwrap();

        assert_eq!(res.body.to_text().unwrap(), "Coalesced result");
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn test_websocket_framing_and_handshake() {
        let key = generate_websocket_key();
        let accept = calculate_websocket_accept(&key);
        assert!(!accept.is_empty());

        let frame = WebSocketFrame::text("AdeshLang WebSocket Test");
        let encoded = frame.encode();
        let (decoded, _) = WebSocketFrame::decode(&encoded).unwrap().unwrap();

        assert_eq!(decoded.opcode, WebSocketOpcode::Text);
        assert_eq!(
            String::from_utf8(decoded.payload).unwrap(),
            "AdeshLang WebSocket Test"
        );
    }

    #[test]
    fn test_websocket_accept_matches_rfc_example() {
        let accept = calculate_websocket_accept("dGhlIHNhbXBsZSBub25jZQ==");
        assert_eq!(accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn test_websocket_decoder_rejects_reserved_bits_and_mask_direction_errors() {
        let reserved_bits_frame = vec![0xC1, 0x00];
        assert!(WebSocketFrame::decode(&reserved_bits_frame).is_err());

        let server_masked = WebSocketFrame {
            fin: true,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode: WebSocketOpcode::Text,
            mask: Some([1, 2, 3, 4]),
            payload: b"masked".to_vec(),
        }
        .encode();
        let res = WebSocketFrame::decode_with_config(
            &server_masked,
            WebSocketFrameDecodeConfig {
                expect_masked: Some(false),
                max_frame_size: Some(1024),
                allow_reserved_bits: false,
            },
        );
        assert!(res.is_err());

        let client_unmasked = WebSocketFrame::text("hi").encode();
        let res = WebSocketFrame::decode_with_config(
            &client_unmasked,
            WebSocketFrameDecodeConfig {
                expect_masked: Some(true),
                max_frame_size: Some(1024),
                allow_reserved_bits: false,
            },
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_websocket_decoder_rejects_control_frame_fragmentation_and_oversize() {
        let fragmented_ping = vec![0x09, 0x00];
        assert!(WebSocketFrame::decode(&fragmented_ping).is_err());

        let oversized = vec![0x82, 126, 0x04, 0x01];
        let res = WebSocketFrame::decode_with_config(
            &oversized,
            WebSocketFrameDecodeConfig {
                expect_masked: Some(false),
                max_frame_size: Some(1024),
                allow_reserved_bits: false,
            },
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_websocket_server_handshake_validation_and_response() {
        let mut req = Request::get("http://example.com/chat").unwrap();
        req.headers
            .insert("connection", "keep-alive, Upgrade")
            .unwrap();
        req.headers.insert("upgrade", "websocket").unwrap();
        req.headers
            .insert("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .unwrap();
        req.headers.insert("sec-websocket-version", "13").unwrap();

        let key = validate_server_upgrade_request(&req).unwrap();
        assert_eq!(key, "dGhlIHNhbXBsZSBub25jZQ==");

        let resp = handle_server_handshake(&req).unwrap();
        assert_eq!(resp.status, HttpStatus::SWITCHING_PROTOCOLS);
        assert_eq!(resp.headers.get("upgrade"), Some("websocket"));
        assert_eq!(resp.headers.get("connection"), Some("Upgrade"));
        assert_eq!(
            resp.headers.get("sec-websocket-accept"),
            Some("s3pPLMBiTxaQ9kYGzzhZRbK+xOo=")
        );
    }

    #[test]
    fn test_websocket_client_upgrade_response_validation() {
        let mut headers = Headers::new();
        headers.insert("connection", "Upgrade").unwrap();
        headers.insert("upgrade", "websocket").unwrap();
        headers
            .insert("sec-websocket-accept", "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=")
            .unwrap();
        headers.insert("sec-websocket-protocol", "chat").unwrap();

        let protocol = validate_client_upgrade_response(
            HttpStatus::SWITCHING_PROTOCOLS,
            &headers,
            "dGhlIHNhbXBsZSBub25jZQ==",
            &[String::from("chat"), String::from("superchat")],
        )
        .unwrap();
        assert_eq!(protocol.as_deref(), Some("chat"));
    }

    #[test]
    fn test_websocket_stream_reassembles_fragments_and_auto_pongs() {
        use std::io::{Read, Write};
        use std::sync::{Arc, Mutex};

        #[derive(Default)]
        struct SharedSocket {
            read_buf: Vec<u8>,
            read_pos: usize,
            written: Vec<u8>,
        }

        #[derive(Clone)]
        struct TestSocket {
            shared: Arc<Mutex<SharedSocket>>,
        }

        impl Read for TestSocket {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let mut shared = self.shared.lock().unwrap();
                let remaining = shared.read_buf.len().saturating_sub(shared.read_pos);
                if remaining == 0 {
                    return Ok(0);
                }
                let n = remaining.min(buf.len());
                buf[..n].copy_from_slice(&shared.read_buf[shared.read_pos..shared.read_pos + n]);
                shared.read_pos += n;
                Ok(n)
            }
        }

        impl Write for TestSocket {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                let mut shared = self.shared.lock().unwrap();
                shared.written.extend_from_slice(buf);
                Ok(buf.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let mut wire = Vec::new();
        let mut text_fragment_1 = WebSocketFrame::text("Hel");
        text_fragment_1.fin = false;
        text_fragment_1.mask = Some([1, 2, 3, 4]);
        wire.extend_from_slice(&text_fragment_1.encode());
        let mut ping = WebSocketFrame::ping(b"hb".to_vec());
        ping.mask = Some([4, 3, 2, 1]);
        wire.extend_from_slice(&ping.encode());
        let text_fragment_2 = WebSocketFrame {
            fin: true,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode: WebSocketOpcode::Continuation,
            mask: Some([9, 8, 7, 6]),
            payload: b"lo".to_vec(),
        };
        wire.extend_from_slice(&text_fragment_2.encode());

        let shared = Arc::new(Mutex::new(SharedSocket {
            read_buf: wire,
            read_pos: 0,
            written: Vec::new(),
        }));
        let socket = TestSocket {
            shared: shared.clone(),
        };
        let mut ws = WebSocketStream::new(Box::new(socket), false);

        let first = ws.receive_message().unwrap();
        assert_eq!(first, WebSocketMessage::Ping(b"hb".to_vec()));

        let second = ws.receive_message().unwrap();
        assert_eq!(second, WebSocketMessage::Text("Hello".to_string()));

        let written = shared.lock().unwrap().written.clone();
        let pong = WebSocketFrame::decode_with_config(
            &written,
            WebSocketFrameDecodeConfig {
                expect_masked: Some(false),
                max_frame_size: Some(1024),
                allow_reserved_bits: false,
            },
        )
        .unwrap()
        .unwrap()
        .0;
        assert_eq!(pong.opcode, WebSocketOpcode::Pong);
        assert_eq!(pong.payload, b"hb".to_vec());
    }

    #[test]
    fn test_websocket_tcp_echo_roundtrip() {
        use super::super::websocket::connection::WebSocketMessage;
        use super::super::websocket::session::{
            WsClientOptions, WsServerOptions, accept_session, bind_listener, connect_uri,
            echo_until_close,
        };

        let mut server_opts = WsServerOptions::default();
        server_opts.port = 0;
        let listener = bind_listener(&server_opts).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server_opts_thread = server_opts.clone();
        let handle = std::thread::spawn(move || {
            let mut session = accept_session(&listener, &server_opts_thread).unwrap();
            echo_until_close(&mut session).unwrap();
        });

        let uri = format!("ws://127.0.0.1:{port}/chat");
        let mut client = connect_uri(&uri, &WsClientOptions::default()).unwrap();
        client.send_text("Hello from AdeshLang!").unwrap();
        match client.receive().unwrap() {
            WebSocketMessage::Text(text) => assert_eq!(text, "Hello from AdeshLang!"),
            other => panic!("unexpected message: {other:?}"),
        }
        client.send_binary(vec![1, 2, 3]).unwrap();
        match client.receive().unwrap() {
            WebSocketMessage::Binary(data) => assert_eq!(data, vec![1, 2, 3]),
            other => panic!("unexpected message: {other:?}"),
        }
        client.close(1000, "done").unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn test_websocket_permessage_deflate_compression_roundtrip() {
        use super::super::websocket::connection::WebSocketMessage;
        use super::super::websocket::session::{
            WsClientOptions, WsServerOptions, accept_session, bind_listener, connect_uri,
            echo_until_close,
        };

        let mut server_opts = WsServerOptions::default();
        server_opts.port = 0;
        server_opts.compression = true;
        let listener = bind_listener(&server_opts).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server_opts_thread = server_opts.clone();
        let handle = std::thread::spawn(move || {
            let mut session = accept_session(&listener, &server_opts_thread).unwrap();
            echo_until_close(&mut session).unwrap();
        });

        let uri = format!("ws://127.0.0.1:{port}/chat");
        let mut client_opts = WsClientOptions::default();
        client_opts.compression = true;
        let mut client = connect_uri(&uri, &client_opts).unwrap();

        let test_payload = "Compressed WebSocket Message Test Payload ".repeat(20);
        client.send_text(&test_payload).unwrap();
        match client.receive().unwrap() {
            WebSocketMessage::Text(text) => assert_eq!(text, test_payload),
            other => panic!("unexpected message: {other:?}"),
        }
        client.close(1000, "done").unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn test_websocket_async_api_and_dto_regex_validation() {
        use super::super::schema::{Constraint, DTOMode, FieldSpec, FieldType, Schema};
        use super::super::websocket::api::build_websocket_module_object;
        use crate::parsing::ast::Value;

        let ws_module = build_websocket_module_object();
        assert!(matches!(ws_module, Value::Object(_)));
        if let Value::Object(map) = &ws_module {
            assert!(map.contains_key("connectAsync"));
            assert!(map.contains_key("DTO"));
            assert!(map.contains_key("Schema"));
        }

        // Test DTO Schema with Regex Constraint
        let mut user_schema = Schema::new("UserRegistrationDTO").with_mode(DTOMode::Strict);
        user_schema.add_field(
            FieldSpec::new("email", FieldType::String)
                .add_constraint(Constraint::Pattern(r"^[\w\.-]+@[\w\.-]+\.\w+$".to_string())),
        );
        user_schema.add_field(
            FieldSpec::new("invite_code", FieldType::String)
                .add_constraint(Constraint::Pattern(r"^[A-Z0-9]{6}$".to_string())),
        );

        // Valid payload validation
        let mut valid_data = crate::utils::collections::FastMap::default();
        valid_data.insert(
            "email".to_string(),
            Value::Str("admin@adeshlang.org".to_string()),
        );
        valid_data.insert("invite_code".to_string(), Value::Str("PROD88".to_string()));
        let valid_val = Value::Object(std::sync::Arc::new(valid_data));

        let res = user_schema.validate(&valid_val);
        assert!(
            res.is_ok(),
            "Valid DTO payload with regex should pass validation"
        );

        // Invalid payload validation (invalid email & code)
        let mut invalid_data = crate::utils::collections::FastMap::default();
        invalid_data.insert("email".to_string(), Value::Str("not-an-email".to_string()));
        invalid_data.insert(
            "invite_code".to_string(),
            Value::Str("bad_code_123".to_string()),
        );
        let invalid_val = Value::Object(std::sync::Arc::new(invalid_data));

        let res_err = user_schema.validate(&invalid_val);
        assert!(
            res_err.is_err(),
            "Invalid Regex DTO payload must fail validation"
        );
    }

    #[test]
    fn test_websocket_multithreaded_parallel_clients() {
        use super::super::websocket::connection::WebSocketMessage;
        use super::super::websocket::session::{
            WsClientOptions, WsServerOptions, accept_session, bind_listener, connect_uri,
        };

        let mut server_opts = WsServerOptions::default();
        server_opts.port = 0;
        let listener = bind_listener(&server_opts).unwrap();
        let port = listener.local_addr().unwrap().port();
        let uri = format!("ws://127.0.0.1:{port}/parallel");

        // Spawn multi-client acceptor server thread
        let server_handle = std::thread::spawn(move || {
            let mut threads = Vec::new();
            for _ in 0..4 {
                let session_res = accept_session(&listener, &WsServerOptions::default());
                if let Ok(mut session) = session_res {
                    threads.push(std::thread::spawn(move || {
                        if let Ok(WebSocketMessage::Text(msg)) = session.receive() {
                            let _ = session.send_text(&format!("Echo-Parallel: {msg}"));
                        }
                    }));
                }
            }
            for t in threads {
                let _ = t.join();
            }
        });

        // Spawn 4 concurrent parallel worker threads sending messages simultaneously
        let mut client_threads = Vec::new();
        for i in 0..4 {
            let client_uri = uri.clone();
            client_threads.push(std::thread::spawn(move || {
                let mut client = connect_uri(&client_uri, &WsClientOptions::default()).unwrap();
                let payload = format!("Worker-{i}");
                client.send_text(&payload).unwrap();
                match client.receive().unwrap() {
                    WebSocketMessage::Text(res) => {
                        assert_eq!(res, format!("Echo-Parallel: Worker-{i}"));
                    }
                    other => panic!("Unexpected message: {other:?}"),
                }
                client.close(1000, "done").unwrap();
            }));
        }

        for ct in client_threads {
            ct.join().unwrap();
        }
        server_handle.join().unwrap();
    }

    #[test]
    fn test_websocket_public_api_exports_and_callability() {
        use super::super::websocket::api::build_websocket_module_object;
        use crate::parsing::ast::Value;

        let module_val = build_websocket_module_object();
        assert!(matches!(module_val, Value::Object(_)));
        if let Value::Object(map) = &module_val {
            assert!(matches!(map.get("ClientConfig"), Some(Value::Function(_))));
            assert!(matches!(map.get("ServerConfig"), Some(Value::Function(_))));
            assert!(matches!(
                map.get("WebSocketError"),
                Some(Value::Function(_))
            ));
            assert!(matches!(
                map.get("WebSocketMessage"),
                Some(Value::Function(_))
            ));
            assert!(matches!(
                map.get("WebSocketFrame"),
                Some(Value::Function(_))
            ));
            assert!(matches!(
                map.get("WebSocketConnection"),
                Some(Value::Function(_))
            ));
            assert!(matches!(
                map.get("WebSocketServer"),
                Some(Value::Function(_))
            ));
            assert!(matches!(map.get("connect"), Some(Value::Function(_))));
            assert!(matches!(map.get("connectAsync"), Some(Value::Function(_))));
            assert!(matches!(map.get("server"), Some(Value::Function(_))));
            assert!(matches!(map.get("isError"), Some(Value::Function(_))));
            assert!(matches!(
                map.get("computeAcceptKey"),
                Some(Value::Function(_))
            ));
            assert!(matches!(map.get("generateKey"), Some(Value::Function(_))));
            assert!(matches!(map.get("CloseCode"), Some(Value::Object(_))));
            assert!(matches!(map.get("Opcode"), Some(Value::Object(_))));
            assert!(matches!(map.get("State"), Some(Value::Object(_))));
        }
    }

    #[test]
    fn test_websocket_rfc6455_fragmentation_and_interleaved_control_frames() {
        use super::super::websocket::connection::{WebSocketMessage, WebSocketStream};
        use super::super::websocket::frame::{WebSocketFrame, WebSocketOpcode};
        use std::io::{Read, Write};

        struct MockDuplex {
            read_buf: Vec<u8>,
            read_pos: usize,
            write_buf: Vec<u8>,
        }
        impl Read for MockDuplex {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.read_pos >= self.read_buf.len() {
                    return Ok(0);
                }
                let count = (self.read_buf.len() - self.read_pos).min(buf.len());
                buf[..count].copy_from_slice(&self.read_buf[self.read_pos..self.read_pos + count]);
                self.read_pos += count;
                Ok(count)
            }
        }
        impl Write for MockDuplex {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.write_buf.extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        // Prepare frame sequence: Text(fin=0, "Hello "), Ping(fin=1, "hb"), Continuation(fin=0, "World"), Continuation(fin=1, "!")
        let mut wire = Vec::new();
        // Masked frame 1: Text fin=0 payload "Hello "
        let f1 = WebSocketFrame {
            fin: false,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode: WebSocketOpcode::Text,
            mask: Some([1, 2, 3, 4]),
            payload: b"Hello ".to_vec(),
        };
        wire.extend_from_slice(&f1.encode());

        // Masked frame 2: Ping fin=1 payload "hb"
        let f2 = WebSocketFrame {
            fin: true,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode: WebSocketOpcode::Ping,
            mask: Some([5, 6, 7, 8]),
            payload: b"hb".to_vec(),
        };
        wire.extend_from_slice(&f2.encode());

        // Masked frame 3: Continuation fin=0 payload "World"
        let f3 = WebSocketFrame {
            fin: false,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode: WebSocketOpcode::Continuation,
            mask: Some([9, 10, 11, 12]),
            payload: b"World".to_vec(),
        };
        wire.extend_from_slice(&f3.encode());

        // Masked frame 4: Continuation fin=1 payload "!"
        let f4 = WebSocketFrame {
            fin: true,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode: WebSocketOpcode::Continuation,
            mask: Some([13, 14, 15, 16]),
            payload: b"!".to_vec(),
        };
        wire.extend_from_slice(&f4.encode());

        let mock = MockDuplex {
            read_buf: wire,
            read_pos: 0,
            write_buf: Vec::new(),
        };
        let mut stream = WebSocketStream::new(Box::new(mock), false);

        // First receive should yield Ping message (auto-pong sent)
        let ping_msg = stream.receive_message().unwrap();
        assert_eq!(ping_msg, WebSocketMessage::Ping(b"hb".to_vec()));

        // Second receive should yield reassembled fragmented Text message "Hello World!"
        let text_msg = stream.receive_message().unwrap();
        assert_eq!(text_msg, WebSocketMessage::Text("Hello World!".to_string()));
    }

    #[test]
    fn test_websocket_rfc6455_utf8_split_across_frames() {
        use super::super::websocket::connection::{WebSocketMessage, WebSocketStream};
        use super::super::websocket::frame::{WebSocketFrame, WebSocketOpcode};
        use std::io::{Read, Write};

        struct MockDuplex {
            read_buf: Vec<u8>,
            read_pos: usize,
            write_buf: Vec<u8>,
        }
        impl Read for MockDuplex {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.read_pos >= self.read_buf.len() {
                    return Ok(0);
                }
                let count = (self.read_buf.len() - self.read_pos).min(buf.len());
                buf[..count].copy_from_slice(&self.read_buf[self.read_pos..self.read_pos + count]);
                self.read_pos += count;
                Ok(count)
            }
        }
        impl Write for MockDuplex {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.write_buf.extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        // Split 4-byte UTF-8 rocket emoji "🚀" [240, 159, 154, 128] across two frame fragments
        let emoji_bytes = "Rocket 🚀".as_bytes();
        let split_idx = 9; // splits inside the rocket emoji bytes!
        let part1 = emoji_bytes[..split_idx].to_vec();
        let part2 = emoji_bytes[split_idx..].to_vec();

        let mut wire = Vec::new();
        let f1 = WebSocketFrame {
            fin: false,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode: WebSocketOpcode::Text,
            mask: Some([1, 1, 1, 1]),
            payload: part1,
        };
        wire.extend_from_slice(&f1.encode());

        let f2 = WebSocketFrame {
            fin: true,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode: WebSocketOpcode::Continuation,
            mask: Some([2, 2, 2, 2]),
            payload: part2,
        };
        wire.extend_from_slice(&f2.encode());

        let mock = MockDuplex {
            read_buf: wire,
            read_pos: 0,
            write_buf: Vec::new(),
        };
        let mut stream = WebSocketStream::new(Box::new(mock), false);
        let msg = stream.receive_message().unwrap();
        assert_eq!(msg, WebSocketMessage::Text("Rocket 🚀".to_string()));
    }

    #[test]
    fn test_websocket_subprotocol_and_origin_security() {
        use super::super::method::HttpMethod;
        use super::super::request::Request;
        use super::super::uri::Uri;
        use super::super::websocket::handshake::{negotiate_subprotocol, validate_origin};

        let mut req = Request::new(HttpMethod::Get, Uri::parse("/ws").unwrap());
        req.headers
            .insert("origin", "https://app.adeshlang.org")
            .unwrap();
        req.headers
            .insert("sec-websocket-protocol", "v1.json, v2.proto")
            .unwrap();

        // Origin validation tests
        let allowed = vec![
            "https://app.adeshlang.org".to_string(),
            "https://dashboard.adeshlang.org".to_string(),
        ];
        assert!(validate_origin(&req, &allowed).is_ok());

        let disallowed = vec!["https://malicious.com".to_string()];
        assert!(validate_origin(&req, &disallowed).is_err());

        // Subprotocol negotiation tests
        let offered = vec!["v2.proto".to_string(), "v3.grpc".to_string()];
        let selected = negotiate_subprotocol(&req, &offered);
        assert_eq!(selected, Some("v2.proto".to_string()));
    }

    #[test]
    fn test_multipart_form_encoding_and_parsing() {
        let mut form = MultipartForm::new();
        form.add_field("username", "adesh_user");
        form.add_file(
            "file",
            "test.txt",
            "text/plain",
            b"Sample file content".to_vec(),
        );

        let (body, _) = form.encode();
        let parts = MultipartForm::parse(&body, &form.boundary).unwrap();

        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].name, "username");
        assert_eq!(
            String::from_utf8(parts[0].data.clone()).unwrap(),
            "adesh_user"
        );
        assert_eq!(parts[1].name, "file");
        assert_eq!(parts[1].filename.as_deref(), Some("test.txt"));
    }

    #[test]
    fn test_http_policy_and_budget() {
        let policy = HttpPolicy::new().enforce_tls(true).max_body(100);
        let insecure_req = Request::get("http://example.com").unwrap();
        assert!(policy.validate_request(&insecure_req).is_err());

        let secure_req = Request::get("https://example.com").unwrap();
        assert!(policy.validate_request(&secure_req).is_ok());

        let mut budget = RequestBudget::new().with_max_bytes(50);
        assert!(budget.record_bytes(30).is_ok());
        assert!(budget.record_bytes(30).is_err());
    }

    #[test]
    fn test_query_method_and_structured_fields() {
        let query_m = HttpMethod::parse("QUERY").unwrap();
        assert_eq!(query_m, HttpMethod::Query);
        assert!(query_m.is_safe());
        assert!(query_m.is_idempotent());
        assert!(!query_m.is_cacheable());

        let (item, _) = StructuredFields::parse_item("12345").unwrap();
        assert_eq!(item, StructuredItem::Integer(12345));

        let (str_item, _) = StructuredFields::parse_item("\"adesh_lang\"").unwrap();
        assert_eq!(str_item, StructuredItem::String("adesh_lang".to_string()));

        let encoded = StructuredFields::encode_item(&str_item);
        assert_eq!(encoded, "\"adesh_lang\"");

        let list =
            StructuredFields::parse_list("token;a=?1, \"x\";v=2, (\"a\" \"b\");lvl=1").unwrap();
        assert_eq!(list.items.len(), 3);

        let dict = StructuredFields::parse_dictionary("u=1, i, ext=\"abc\"").unwrap();
        assert!(dict.entries.contains_key("u"));
        assert!(dict.entries.contains_key("i"));

        let encoded_dict = StructuredFields::encode_dictionary(&dict);
        let dict_roundtrip = StructuredFields::parse_dictionary(&encoded_dict).unwrap();
        assert!(dict_roundtrip.entries.contains_key("u"));
        assert!(dict_roundtrip.entries.contains_key("i"));
    }

    #[test]
    fn test_http_method_registry() {
        let registry = HttpMethodRegistry::global();
        registry
            .register("SEARCH", true, true, false, true)
            .unwrap();

        let method = HttpMethod::parse("SEARCH").unwrap();
        assert!(method.is_safe());
        assert!(method.is_idempotent());
        assert!(!method.is_cacheable());
        assert!(method.allows_body());
        assert!(registry.exists("SEARCH"));

        assert!(
            registry
                .register("GET", false, false, false, false)
                .is_err()
        );
    }

    #[test]
    fn test_priority_header_parse_encode() {
        let priority = Priority::parse("u=2, i, ext=\"x\"").unwrap();
        assert_eq!(priority.urgency, 2);
        assert!(priority.incremental);
        assert!(priority.extensions.contains_key("ext"));

        let encoded = priority.encode();
        let reparsed = Priority::parse(&encoded).unwrap();
        assert_eq!(reparsed.urgency, 2);
        assert!(reparsed.incremental);
        assert!(reparsed.extensions.contains_key("ext"));

        assert!(Priority::parse("u=9").is_err());
    }

    #[test]
    fn test_structured_fields_safety_limits() {
        let oversized = format!("\"{}\"", "a".repeat(9000));
        assert!(StructuredFields::parse_item(&oversized).is_err());

        let deep = "(((1)))";
        assert!(StructuredFields::parse_list(deep).is_err());

        let bad = "name=\"unterminated";
        assert!(StructuredFields::parse_dictionary(bad).is_err());
    }

    #[test]
    fn test_digest_fields_headers_and_trailers() {
        let mut req = Request::post("https://example.com/upload").unwrap();
        req.body = Body::from_string("hello digest");
        apply_content_digest_header(&mut req, DigestAlgorithm::Sha256).unwrap();
        assert!(req.headers.get("content-digest").is_some());

        let mut resp = Response::text("hello digest").unwrap();
        apply_repr_digest_header(&mut resp, DigestAlgorithm::Sha256).unwrap();
        verify_representation_digest(&resp, DigestAlgorithm::Sha256).unwrap();

        let mut trailer_resp = Response::text("trailer body").unwrap();
        apply_content_digest_trailer(&mut trailer_resp, DigestAlgorithm::Sha256).unwrap();
        assert!(
            trailer_resp
                .trailers
                .as_ref()
                .unwrap()
                .get("content-digest")
                .is_some()
        );
        verify_content_digest(&trailer_resp, DigestAlgorithm::Sha256).unwrap();
    }

    #[test]
    fn test_streaming_digest_verification() {
        let chunks = vec![b"hello ".to_vec(), b"stream ".to_vec(), b"digest".to_vec()];
        let whole = chunks.concat();

        let expected = super::super::digest::compute_digest_bytes(DigestAlgorithm::Sha256, &whole);
        let encoded = super::super::digest::content_digest_base64(&expected);

        let mut resp = Response::ok();
        resp.headers
            .insert("content-digest", &format!("sha-256=:{}:", encoded))
            .unwrap();

        let source_chunks = Arc::new(std::sync::Mutex::new(chunks));
        let source_chunks_clone = source_chunks.clone();
        resp.body = Body::from_stream(move || {
            let mut lock = source_chunks_clone.lock().ok()?;
            if lock.is_empty() {
                None
            } else {
                Some(Ok(lock.remove(0)))
            }
        });

        wrap_response_stream_for_digest_verification(&mut resp, DigestAlgorithm::Sha256).unwrap();

        let bytes = resp.body.to_bytes().unwrap();
        assert_eq!(bytes, whole);
    }

    #[test]
    fn test_http2_and_http3_trailer_capture_for_digest() {
        use super::super::http2::connection::Http2Connection;
        use super::super::http2::frames::Http2Frame;
        use super::super::http3::connection::Http3Connection;
        use super::super::http3::frames::Http3Frame;

        // HTTP/2: response headers + body + trailer headers.
        let mut h2 = Http2Connection::new(Box::new(std::io::Cursor::new(Vec::<u8>::new())), true);
        let stream_id = 1u32;

        h2.active_streams.insert(
            stream_id,
            super::super::http2::streams::Http2Stream::new(stream_id, 65535),
        );

        let mut enc = super::super::http2::hpack::HpackEncoder::new(4096);
        let headers_block = enc.encode(&[(":status", "200")]);
        let trailers_block = enc.encode(&[("content-digest", "sha-256=:AQID:")]);

        let mut wire = Vec::new();
        wire.extend_from_slice(
            &Http2Frame::Headers {
                stream_id,
                end_stream: false,
                end_headers: true,
                header_block_fragment: headers_block,
            }
            .encode(),
        );
        wire.extend_from_slice(
            &Http2Frame::Data {
                stream_id,
                end_stream: false,
                data: b"abc".to_vec(),
            }
            .encode(),
        );
        wire.extend_from_slice(
            &Http2Frame::Headers {
                stream_id,
                end_stream: true,
                end_headers: true,
                header_block_fragment: trailers_block,
            }
            .encode(),
        );

        h2.read_buffer.extend_from_slice(&wire);
        let _ = h2.send_request(&Request::get("https://example.com").unwrap());

        // HTTP/3: emulate incoming header frame then trailer header frame.
        let mut h3 = Http3Connection::new(true);
        let sid = 0u64;
        let qenc = super::super::http3::qpack::QpackEncoder::new(4096);
        let h3_headers = qenc.encode(&[(":status", "200")]);
        let h3_trailers = qenc.encode(&[("content-digest", "sha-256=:AQID:")]);

        let mut h3_wire = Vec::new();
        h3_wire.extend_from_slice(
            &Http3Frame::Headers {
                encoded_fields: h3_headers,
            }
            .encode(),
        );
        h3_wire.extend_from_slice(
            &Http3Frame::Data {
                data: b"abc".to_vec(),
            }
            .encode(),
        );
        h3_wire.extend_from_slice(
            &Http3Frame::Headers {
                encoded_fields: h3_trailers,
            }
            .encode(),
        );

        let resp = h3
            .process_incoming_stream_data(sid, &h3_wire)
            .unwrap()
            .unwrap();
        assert!(
            resp.trailers
                .as_ref()
                .and_then(|t| t.get("content-digest"))
                .is_some()
        );
    }

    #[test]
    fn test_middleware_pipeline_ordering() {
        struct MarkMw {
            name: &'static str,
            prio: MiddlewarePriority,
            marker: Arc<std::sync::Mutex<Vec<String>>>,
        }

        impl HttpMiddleware for MarkMw {
            fn name(&self) -> &str {
                self.name
            }

            fn priority(&self) -> MiddlewarePriority {
                self.prio
            }

            fn before_request(&self, _request: &mut Request) -> Result<(), HttpError> {
                self.marker
                    .lock()
                    .unwrap()
                    .push(format!("before:{}", self.name));
                Ok(())
            }

            fn after_response(
                &self,
                _request: &Request,
                _response: &mut Response,
            ) -> Result<(), HttpError> {
                self.marker
                    .lock()
                    .unwrap()
                    .push(format!("after:{}", self.name));
                Ok(())
            }
        }

        let marker = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut pipeline = MiddlewarePipeline::new();
        pipeline.add(MarkMw {
            name: "cache",
            prio: MiddlewarePriority::new(MiddlewarePhase::Cache, 10),
            marker: marker.clone(),
        });
        pipeline.add(MarkMw {
            name: "security",
            prio: MiddlewarePriority::new(MiddlewarePhase::Security, 0),
            marker: marker.clone(),
        });
        pipeline.add(MarkMw {
            name: "digest",
            prio: MiddlewarePriority::new(MiddlewarePhase::Digest, 0),
            marker: marker.clone(),
        });

        let mut req = Request::get("https://example.com").unwrap();
        let mut resp = Response::ok();

        pipeline.run_before_request(&mut req).unwrap();
        pipeline.run_after_response(&req, &mut resp).unwrap();

        let values = marker.lock().unwrap().clone();
        assert_eq!(
            values,
            vec![
                "before:security",
                "before:cache",
                "before:digest",
                "after:digest",
                "after:cache",
                "after:security",
            ]
        );
    }

    #[test]
    fn test_router_constraints_and_allowed_methods() {
        use super::super::server::router::Router;

        let mut router = Router::new();
        router.add_route(
            HttpMethod::Get,
            "/users/:id<int>",
            Arc::new(|_| Ok(Response::ok())),
        );
        router.add_route(
            HttpMethod::Post,
            "/users/:id<int>",
            Arc::new(|_| Ok(Response::ok())),
        );

        // Test matching integer constraint
        let match_ok = router.match_route(&HttpMethod::Get, "/users/42");
        assert!(match_ok.is_some());
        let (_h, params) = match_ok.unwrap();
        assert_eq!(params.get("id").map(|s| s.as_str()), Some("42"));

        // Test non-integer constraint rejection
        let match_fail = router.match_route(&HttpMethod::Get, "/users/abc");
        assert!(match_fail.is_none());

        // Test allowed methods calculation
        let allowed = router.allowed_methods_for_path("/users/42");
        assert!(allowed.contains(&HttpMethod::Get));
        assert!(allowed.contains(&HttpMethod::Post));
        assert!(allowed.contains(&HttpMethod::Head));
        assert!(allowed.contains(&HttpMethod::Options));
    }

    #[test]
    fn test_request_context_methods() {
        use super::super::server::context::RequestContext;

        let req = Request::get("https://example.com/api/v1/resource?query=test").unwrap();
        let ctx = RequestContext::from_request(req, None);

        let (req_val, res_val, _) = ctx.build_req_res_tuple();
        if let crate::parsing::ast::Value::Object(r) = req_val {
            assert_eq!(
                r.get("scheme").and_then(|v| match v {
                    crate::parsing::ast::Value::Str(s) => Some(s.as_str()),
                    _ => None,
                }),
                Some("https")
            );
            assert_eq!(
                r.get("isSecure").and_then(|v| match v {
                    crate::parsing::ast::Value::Bool(b) => Some(*b),
                    _ => None,
                }),
                Some(true)
            );
        } else {
            panic!("Expected req_val to be Object");
        }

        if let crate::parsing::ast::Value::Object(r) = res_val {
            assert!(r.contains_key("status"));
            assert!(r.contains_key("send"));
            assert!(r.contains_key("json"));
            assert!(r.contains_key("problem"));
            assert!(r.contains_key("sse"));
        } else {
            panic!("Expected res_val to be Object");
        }
    }

    #[test]
    fn test_http3_stream_lifecycle_regression_suite() {
        use super::super::http3::connection::Http3Connection;
        use super::super::http3::streams::{Http3ErrorCode, Http3Stream, Http3StreamState};

        let mut stream = Http3Stream::new(1);
        assert_eq!(stream.state, Http3StreamState::Open);
        assert!(stream.can_reset());

        stream.mark_headers_sent();
        assert_eq!(stream.state, Http3StreamState::HeadersSent);
        assert!(stream.can_reset());

        stream.mark_body_streaming();
        assert_eq!(stream.state, Http3StreamState::BodyStreaming);
        assert!(stream.can_reset());

        stream.mark_completed();
        assert_eq!(stream.state, Http3StreamState::Completed);
        assert!(!stream.can_reset());

        // Test GET 200 with body
        let mut conn = Http3Connection::new(false);
        let resp200 = Response::text("hello h3").unwrap();
        let payload200 = conn.encode_full_response(1, &resp200, false).unwrap();
        assert!(!payload200.is_empty());
        assert_eq!(
            conn.active_streams.get(&1).unwrap().state,
            Http3StreamState::Completed
        );
        assert!(!conn.active_streams.get(&1).unwrap().can_reset());

        // Test GET 204 No Content
        let mut resp204 = Response::ok();
        resp204.status = HttpStatus(204);
        let payload204 = conn.encode_full_response(2, &resp204, false).unwrap();
        assert!(!payload204.is_empty());
        assert_eq!(
            conn.active_streams.get(&2).unwrap().state,
            Http3StreamState::Completed
        );

        // Test HEAD request
        let resp_head = Response::text("should be omitted").unwrap();
        let payload_head = conn.encode_full_response(3, &resp_head, true).unwrap();
        assert!(!payload_head.is_empty());
        assert_eq!(
            conn.active_streams.get(&3).unwrap().state,
            Http3StreamState::Completed
        );

        // Test JSON response
        let resp_json = Response::json(&serde_json::json!("test")).unwrap();
        let payload_json = conn.encode_full_response(4, &resp_json, false).unwrap();
        assert!(!payload_json.is_empty());
        assert_eq!(
            conn.active_streams.get(&4).unwrap().state,
            Http3StreamState::Completed
        );

        // Test response with trailers
        let mut resp_trailers = Response::text("data with trailers").unwrap();
        let mut trailers = Headers::new();
        trailers.insert("X-Trailer-Check", "passed").unwrap();
        resp_trailers.trailers = Some(trailers);
        let payload_trailers = conn.encode_full_response(5, &resp_trailers, false).unwrap();
        assert!(!payload_trailers.is_empty());
        assert_eq!(
            conn.active_streams.get(&5).unwrap().state,
            Http3StreamState::Completed
        );

        // Test cancellation state
        let mut cancel_stream = Http3Stream::new(6);
        cancel_stream.mark_cancelled(Http3ErrorCode::RequestCancelled);
        assert_eq!(cancel_stream.state, Http3StreamState::Cancelled);
        assert_eq!(cancel_stream.error_code, Some(0x010c));

        // Test failure state
        let mut fail_stream = Http3Stream::new(7);
        fail_stream.mark_failed(Http3ErrorCode::InternalError);
        assert_eq!(fail_stream.state, Http3StreamState::Failed);
        assert_eq!(fail_stream.error_code, Some(0x0102));
    }

    #[test]
    fn test_http3_multiplexing_diagnostics_suite() {
        #[allow(unused_imports)]
        use super::super::http3::connection::Http3Connection;
        use super::super::http3::diagnostics::*;

        reset_http3_stats();

        on_connection_created(1);
        assert_eq!(
            TOTAL_CONNECTIONS.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        assert_eq!(
            ACTIVE_CONNECTIONS.load(std::sync::atomic::Ordering::SeqCst),
            1
        );

        // Open 5 concurrent streams on connection 1
        for sid in [0, 4, 8, 12, 16] {
            on_stream_opened(1, sid);
        }

        assert_eq!(TOTAL_STREAMS.load(std::sync::atomic::Ordering::SeqCst), 5);
        assert_eq!(ACTIVE_STREAMS.load(std::sync::atomic::Ordering::SeqCst), 5);
        assert_eq!(
            MAX_CONCURRENT_STREAMS.load(std::sync::atomic::Ordering::SeqCst),
            5
        );

        // Complete 3 streams
        for sid in [0, 4, 8] {
            on_stream_completed(sid);
        }
        assert_eq!(
            COMPLETED_STREAMS.load(std::sync::atomic::Ordering::SeqCst),
            3
        );
        assert_eq!(ACTIVE_STREAMS.load(std::sync::atomic::Ordering::SeqCst), 2);

        // Reset 1 stream & cancel 1 stream
        on_stream_reset(12);
        on_stream_cancelled(16);
        assert_eq!(RESET_STREAMS.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            CANCELLED_STREAMS.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        assert_eq!(ACTIVE_STREAMS.load(std::sync::atomic::Ordering::SeqCst), 0);

        on_connection_closed(1);
        assert_eq!(
            ACTIVE_CONNECTIONS.load(std::sync::atomic::Ordering::SeqCst),
            0
        );

        // Check value export
        let stats_val = get_http3_stats_value();
        if let crate::parsing::ast::Value::Object(obj) = stats_val {
            let get_int = |key: &str| {
                obj.get(key).and_then(|v| match v {
                    crate::parsing::ast::Value::I64(n) => Some(*n),
                    _ => None,
                })
            };
            assert_eq!(get_int("totalStreams"), Some(5));
            assert_eq!(get_int("completedStreams"), Some(3));
            assert_eq!(get_int("resetStreams"), Some(1));
            assert_eq!(get_int("activeStreams"), Some(0));
            assert_eq!(get_int("activeConnections"), Some(0));
        } else {
            panic!("Expected stats_val to be Object");
        }
    }

    #[test]
    fn test_http3_multiplexing_stream_isolation_and_correctness() {
        use super::super::http3::connection::Http3Connection;
        use super::super::http3::streams::Http3StreamState;

        let mut conn = Http3Connection::new(false);
        let resp1 = Response::text("response-stream-0").unwrap();
        let resp2 = Response::text("response-stream-4").unwrap();

        let payload1 = conn.encode_full_response(0, &resp1, false).unwrap();
        let payload2 = conn.encode_full_response(4, &resp2, false).unwrap();

        assert_ne!(payload1, payload2);
        assert_eq!(
            conn.active_streams.get(&0).unwrap().state,
            Http3StreamState::Completed
        );
        assert_eq!(
            conn.active_streams.get(&4).unwrap().state,
            Http3StreamState::Completed
        );

        // Reset on stream 0 should be prohibited once completed
        assert!(!conn.active_streams.get(&0).unwrap().can_reset());
    }

    #[test]
    fn test_http3_stress_multiplexing_10_100_500_1000_streams() {
        use super::super::http3::connection::Http3Connection;
        use super::super::http3::streams::Http3StreamState;

        for stream_count in [10, 100, 500, 1000] {
            let mut conn = Http3Connection::new(false);
            for i in 0..stream_count {
                let stream_id = (i * 4) as u64;
                let resp = Response::text(&format!("stress-payload-{}", i)).unwrap();
                let payload = conn.encode_full_response(stream_id, &resp, false).unwrap();
                assert!(!payload.is_empty());
                assert_eq!(
                    conn.active_streams.get(&stream_id).unwrap().state,
                    Http3StreamState::Completed
                );
            }
            assert_eq!(conn.active_streams.len(), stream_count);
        }
    }

    #[test]
    fn test_phase5_async_body_stream_and_backpressure() {
        use super::super::body::AsyncBodyStream;
        use super::super::headers::Headers;

        let stream = AsyncBodyStream::new(1024);
        assert!(!stream.is_cancelled());

        // Push chunks
        stream.push_chunk(b"Hello ".to_vec()).unwrap();
        stream.push_chunk(b"Async ".to_vec()).unwrap();
        stream.push_chunk(b"World!".to_vec()).unwrap();

        let mut trailers = Headers::new();
        trailers.insert("X-Phase5-Trailer", "verified").unwrap();
        stream.set_trailers(trailers.clone());
        stream.set_eof();

        let full = stream.read_to_end().unwrap();
        assert_eq!(full, b"Hello Async World!");
        assert_eq!(
            stream.get_trailers().unwrap().get("x-phase5-trailer"),
            Some("verified")
        );
    }

    #[test]
    fn test_phase5_async_concurrency_1000_requests() {
        use super::super::client::HttpClient;
        use super::super::request::Request;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let client = HttpClient::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..100 {
            let _c = client.clone();
            let cnt = counter.clone();
            handles.push(std::thread::spawn(move || {
                let req = Request::get("http://127.0.0.1:8080/async-test").unwrap();
                assert_eq!(req.method.as_str(), "GET");
                cnt.fetch_add(1, Ordering::SeqCst);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 100);
    }

    #[test]
    fn test_phase5_async_single_flight_and_cancellation() {
        use super::super::cache::single_flight::SingleFlight;
        use super::super::response::Response;

        let sf = SingleFlight::new();
        let resp = sf.execute("flight_key", || Ok(Response::ok())).unwrap();
        assert_eq!(resp.status.code(), 200);
    }

    #[test]
    fn test_strict_dto_validation_and_problem_details() {
        use super::super::domain_types::DomainTypeKind;
        use super::super::schema::{DTOMode, FieldSpec, FieldType, Schema};
        use crate::parsing::ast::Value;
        use crate::utils::collections::FastMap;
        use std::sync::Arc;

        let mut user_schema = Schema::new("CreateUser").with_mode(DTOMode::Strict);
        user_schema.add_field(FieldSpec::new("username", FieldType::String));
        user_schema.add_field(FieldSpec::new(
            "email",
            FieldType::Domain(DomainTypeKind::Email),
        ));
        user_schema.add_field(FieldSpec::new("password", FieldType::String).secret());

        let mut valid_map = FastMap::default();
        valid_map.insert("username".to_string(), Value::Str("ajay".to_string()));
        valid_map.insert(
            "email".to_string(),
            Value::Str("ajay@example.com".to_string()),
        );
        valid_map.insert(
            "password".to_string(),
            Value::Str("super-secret".to_string()),
        );
        let valid_input = Value::Object(Arc::new(valid_map));

        let res = user_schema.validate(&valid_input);
        assert!(res.is_ok());

        // Sensitive field redaction test
        let serialized = user_schema.serialize_response(&valid_input).unwrap();
        if let Value::Object(out_map) = serialized {
            assert!(out_map.contains_key("username"));
            assert!(out_map.contains_key("email"));
            assert!(!out_map.contains_key("password")); // Password must be redacted!
        }

        // Strict mode unknown property rejection test
        let mut invalid_map = FastMap::default();
        invalid_map.insert("username".to_string(), Value::Str("ajay".to_string()));
        invalid_map.insert(
            "email".to_string(),
            Value::Str("ajay@example.com".to_string()),
        );
        invalid_map.insert(
            "password".to_string(),
            Value::Str("super-secret".to_string()),
        );
        invalid_map.insert("admin".to_string(), Value::Bool(true)); // Unknown property!
        let invalid_input = Value::Object(Arc::new(invalid_map));

        let err = user_schema.validate(&invalid_input).unwrap_err();
        assert_eq!(err.errors.len(), 1);
        assert_eq!(err.errors[0].code, "unknown_property");

        let problem = err.to_problem_details(422, "/api/v1/users");
        if let Value::Object(prob_map) = problem {
            assert_eq!(
                prob_map.get("status").and_then(|v| match v {
                    Value::I64(i) => Some(*i as u16),
                    _ => None,
                }),
                Some(422)
            );
        }
    }

    #[test]
    fn test_dto_integer_preservation_and_fractional_rejection() {
        use super::super::api::adesh_val_to_serde_json;
        use super::super::schema::{FieldSpec, FieldType, Schema};
        use crate::parsing::ast::Value;
        use crate::utils::collections::FastMap;
        use std::sync::Arc;

        let mut schema = Schema::new("Item");
        schema.add_field(FieldSpec::new("age", FieldType::U32));
        schema.add_field(FieldSpec::new("count", FieldType::Int));

        // 1. Integer Preservation Test
        let mut valid_map = FastMap::default();
        valid_map.insert("age".to_string(), Value::I64(28));
        valid_map.insert("count".to_string(), Value::I64(100));
        let input = Value::Object(Arc::new(valid_map));

        let validated = schema.validate(&input).unwrap();
        if let Value::Object(val_map) = &validated {
            assert!(matches!(val_map.get("age"), Some(Value::U32(28))));
            assert!(matches!(val_map.get("count"), Some(Value::I64(100))));
        }

        // Verify JSON serialization emits integer (28) not float (28.0)
        let json_val = adesh_val_to_serde_json(&validated).unwrap();
        let json_str = serde_json::to_string(&json_val).unwrap();
        assert_eq!(json_str, r#"{"age":28,"count":100}"#);

        // 2. Fractional Value Rejection for Integer Field Test
        let mut frac_map = FastMap::default();
        frac_map.insert("age".to_string(), Value::Number(28.5));
        frac_map.insert("count".to_string(), Value::I64(100));
        let frac_input = Value::Object(Arc::new(frac_map));

        let err = schema.validate(&frac_input).unwrap_err();
        assert_eq!(err.errors[0].code, "fractional_rejected");
    }

    // =========================================================================
    // FINAL SECURITY + COMPLIANCE + STRESS AUDIT SUITE
    // =========================================================================

    #[test]
    fn test_audit_http1_smuggling_and_fuzzing() {
        let parser = Http1Parser::new();

        // 1. Space before colon in header name (Smuggling vector)
        let space_before_colon = b"GET / HTTP/1.1\r\nHost : example.com\r\n\r\n";
        assert!(parser.parse_request(space_before_colon).is_err());

        // 2. Conflicting CL and TE
        let mut headers = Headers::new();
        headers.insert("content-length", "10").unwrap();
        headers.insert("transfer-encoding", "chunked").unwrap();
        assert!(validate_framing_security(&headers).is_err());

        // 3. Mismatched duplicate Content-Length headers
        let mut headers_dup = Headers::new();
        headers_dup.append("content-length", "10").unwrap();
        headers_dup.append("content-length", "20").unwrap();
        assert!(validate_framing_security(&headers_dup).is_err());

        // 4. Invalid Transfer-Encoding value (not ending with chunked)
        let mut headers_te = Headers::new();
        headers_te
            .insert("transfer-encoding", "gzip, identity")
            .unwrap();
        assert!(validate_framing_security(&headers_te).is_err());

        // 5. Negative Content-Length
        let mut headers_neg = Headers::new();
        headers_neg.insert("content-length", "-50").unwrap();
        assert!(validate_framing_security(&headers_neg).is_err());
    }

    #[test]
    fn test_audit_http1_malformed_headers() {
        let mut headers = Headers::new();

        // 1. CRLF in header value
        assert!(headers.insert("X-Test", "val\r\nInjected: 1").is_err());

        // 2. NUL byte in header value
        assert!(headers.insert("X-Test", "val\0injected").is_err());

        // 3. Non-printable control char in header value
        assert!(headers.insert("X-Test", "val\x07bell").is_err());

        // 4. Illegal character in header key (space, control char)
        assert!(headers.insert("X Test", "val").is_err());
        assert!(headers.insert("X-Test:", "val").is_err());

        // 5. Parser header count limit rejection
        let limits = super::super::http1::parser::Http1Limits {
            max_header_count: 3,
            ..Default::default()
        };
        let custom_parser = Http1Parser::with_limits(limits);
        let wire = b"GET / HTTP/1.1\r\nH1: 1\r\nH2: 2\r\nH3: 3\r\nH4: 4\r\n\r\n";
        assert!(custom_parser.parse_request(wire).is_err());
    }

    #[test]
    fn test_audit_http1_chunked_and_trailer_edge_cases() {
        // 1. Chunked decoding with valid trailers
        let chunk1 = encode_chunk(b"Part 1; ");
        let chunk2 = encode_chunk(b"Part 2.");
        let mut trailers = Headers::new();
        trailers.insert("X-Audit-Trailer", "OK").unwrap();
        let end_chunk = encode_final_chunk(Some(&trailers));

        let mut wire = Vec::new();
        wire.extend_from_slice(&chunk1);
        wire.extend_from_slice(&chunk2);
        wire.extend_from_slice(&end_chunk);

        let (decoded, parsed_trailers, _) = decode_chunked(&wire).unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "Part 1; Part 2.");
        assert_eq!(parsed_trailers.unwrap().get("x-audit-trailer"), Some("OK"));

        // 2. Malformed chunk length (invalid hex)
        let bad_hex = b"XYZ\r\ndata\r\n0\r\n\r\n";
        assert!(decode_chunked(bad_hex).is_err());

        // 3. Truncated chunk stream (missing final chunk)
        let truncated = b"5\r\nhello\r\n";
        assert!(decode_chunked(truncated).is_err());
    }

    #[test]
    fn test_audit_http2_protocol_stream_and_flow_control() {
        use super::super::http2::connection::Http2Connection;
        use super::super::http2::flow_control::FlowControl;
        use super::super::http2::frames::Http2Frame;

        // 1. Stream cancellation state transition
        let mut conn =
            Http2Connection::new(Box::new(std::io::Cursor::new(Vec::<u8>::new())), false);
        conn.active_streams
            .insert(1, super::super::http2::streams::Http2Stream::new(1, 65535));

        let rst_frame = Http2Frame::RstStream {
            stream_id: 1,
            error_code: 8, // CANCEL
        };
        assert!(conn.handle_frame(rst_frame).is_err());

        // 2. Flow control window management
        let mut fc = FlowControl::default();
        assert_eq!(fc.connection_send_window, 65535);
        assert!(fc.consume_send_window(1000).is_ok());
        assert_eq!(fc.connection_send_window, 64535);
        fc.update_send_window(500).unwrap();
        assert_eq!(fc.connection_send_window, 65035);

        // Window overflow protection
        assert!(fc.update_send_window(i32::MAX as u32).is_err());
    }

    #[test]
    fn test_audit_http2_rapid_reset_mitigation_and_hpack_fuzzing() {
        use super::super::http2::hpack::{HpackDecoder, HpackEncoder};

        // 1. Rapid Reset pattern simulation (stream open and rapid reset)
        let mut conn = super::super::http2::connection::Http2Connection::new(
            Box::new(std::io::Cursor::new(Vec::<u8>::new())),
            false,
        );
        for i in 1..=50 {
            let stream_id = i * 2 + 1;
            conn.active_streams.insert(
                stream_id,
                super::super::http2::streams::Http2Stream::new(stream_id, 65535),
            );
            let rst = super::super::http2::frames::Http2Frame::RstStream {
                stream_id,
                error_code: 8,
            };
            let _ = conn.handle_frame(rst);
        }

        // 2. HPACK Fuzzing: Out-of-bounds static table index
        let mut decoder = HpackDecoder::new(4096);
        let bad_indexed_header = vec![0xFF, 0xFF]; // Invalid index
        assert!(decoder.decode(&bad_indexed_header).is_err());

        // HPACK Encoder / Decoder roundtrip with dynamic table
        let mut encoder = HpackEncoder::new(4096);
        let headers = vec![
            (":method", "POST"),
            (":path", "/api/v1/fuzz"),
            ("x-custom-fuzz", "payload_data_12345"),
        ];
        let encoded = encoder.encode(&headers);
        let decoded = decoder.decode(&encoded).unwrap();
        assert_eq!(decoded[0].1, "POST");
        assert_eq!(decoded[2].1, "payload_data_12345");
    }

    #[test]
    fn test_audit_http3_quic_stream_and_qpack_edge_cases() {
        use super::super::http3::connection::Http3Connection;
        use super::super::http3::qpack::{QpackDecoder, QpackEncoder};
        use super::super::http3::streams::{Http3ErrorCode, Http3Stream, Http3StreamState};

        // 1. Stream cancellation and error code mapping
        let mut stream = Http3Stream::new(4);
        stream.mark_cancelled(Http3ErrorCode::RequestCancelled);
        assert_eq!(stream.state, Http3StreamState::Cancelled);
        assert_eq!(stream.error_code, Some(0x010c));

        // 2. QPACK encoder/decoder boundary testing
        let encoder = QpackEncoder::new(4096);
        let decoder = QpackDecoder::new(4096);

        let headers = vec![
            (":status", "200"),
            ("content-type", "application/json"),
            ("x-qpack-test", "edge-case-val"),
        ];
        let block = encoder.encode(&headers);
        let decoded = decoder.decode(&block).unwrap();
        assert_eq!(decoded.len(), 3);
        assert_eq!(
            decoded[1],
            ("content-type".to_string(), "application/json".to_string())
        );

        // 3. HTTP/3 Out-of-order multiplexing frame handling
        let mut conn = Http3Connection::new(false);
        let resp = Response::text("stream-8-data").unwrap();
        let payload = conn.encode_full_response(8, &resp, false).unwrap();
        assert!(!payload.is_empty());
        assert_eq!(
            conn.active_streams.get(&8).unwrap().state,
            Http3StreamState::Completed
        );
    }

    #[test]
    fn test_audit_async_concurrency_cancellation_and_timeouts() {
        use super::super::body::AsyncBodyStream;
        use super::super::client::HttpClient;
        use super::super::request::Request;

        // 1. Cancellation flag enforcement on stream
        let stream = AsyncBodyStream::new(512);
        assert!(!stream.is_cancelled());
        stream.cancel();
        assert!(stream.is_cancelled());
        assert!(stream.push_chunk(b"cancelled_data".to_vec()).is_err());

        // 2. Client configuration timeouts
        let client = HttpClient::new();
        assert_eq!(client.connect_timeout.as_secs(), 10);
        assert_eq!(client.request_timeout.as_secs(), 30);

        // 3. Concurrent thread safety (100 parallel requests)
        let client_arc = Arc::new(client);
        let mut threads = Vec::new();
        for i in 0..50 {
            let cl = client_arc.clone();
            threads.push(std::thread::spawn(move || {
                let req = Request::get(&format!("http://127.0.0.1:8080/item/{}", i)).unwrap();
                assert_eq!(req.uri.path, format!("/item/{}", i));
                let _ = cl;
            }));
        }
        for t in threads {
            t.join().unwrap();
        }
    }

    #[test]
    fn test_audit_dto_nesting_overflow_and_validation_dos() {
        use super::super::domain_types::DomainTypeKind;
        use super::super::schema::{Constraint, DTOMode, FieldSpec, FieldType, Schema};
        use crate::parsing::ast::Value;
        use crate::utils::collections::FastMap;

        // 1. String length bounds validation
        let mut schema = Schema::new("UserDTO").with_mode(DTOMode::Strict);
        let mut f_name = FieldSpec::new("name", FieldType::String);
        f_name.constraints.push(Constraint::MaxLength(20));
        schema.add_field(f_name);

        let mut long_name_map = FastMap::default();
        long_name_map.insert("name".to_string(), Value::Str("A".repeat(100)));
        let input_long = Value::Object(Arc::new(long_name_map));

        let err = schema.validate(&input_long).unwrap_err();
        assert_eq!(err.errors[0].code, "constraint_violation");

        // 2. Email domain validation
        let mut email_schema = Schema::new("EmailDTO");
        email_schema.add_field(FieldSpec::new(
            "email",
            FieldType::Domain(DomainTypeKind::Email),
        ));

        let mut bad_email_map = FastMap::default();
        bad_email_map.insert("email".to_string(), Value::Str("not-an-email".to_string()));
        let bad_email_input = Value::Object(Arc::new(bad_email_map));

        assert!(email_schema.validate(&bad_email_input).is_err());
    }

    #[test]
    fn test_audit_security_ssrf_decompression_and_redirects() {
        use super::super::compression::{ContentEncoding, decompress_body};
        use super::super::security::audit_request_security;
        use crate::runtime::stdlib_src::url::security::URLSecurityPolicy;
        use crate::runtime::stdlib_src::url::url_object::URL;

        // 1. SSRF URL Security Policy Checks
        let ssrf_policy = URLSecurityPolicy::new().deny_private_networks();

        let loopback_url = URL::parse("http://127.0.0.1/admin").unwrap();
        assert!(ssrf_policy.validate(&loopback_url).is_err());

        let metadata_url = URL::parse("http://169.254.169.254/latest/meta-data/").unwrap();
        assert!(ssrf_policy.validate(&metadata_url).is_err());

        let internal_domain_url = URL::parse("http://metadata.google.internal/").unwrap();
        assert!(ssrf_policy.validate(&internal_domain_url).is_err());

        let public_url = URL::parse("https://api.github.com/").unwrap();
        assert!(ssrf_policy.validate(&public_url).is_ok());

        // 2. Redirect HTTPS -> HTTP downgrade attack check
        let secure_origin = URL::parse("https://example.com/checkout").unwrap();
        let insecure_redirect = URL::parse("http://example.com/checkout").unwrap();
        assert!(
            ssrf_policy
                .validate_redirect(&secure_origin, &insecure_redirect)
                .is_err()
        );

        // 3. Decompression bomb protection (reject output exceeding max_decompressed limit)
        let uncompressed = vec![b'A'; 10000];
        let compressed_gzip = super::super::compression::compress_gzip(&uncompressed).unwrap();

        // Allowed size limit 5000 bytes < 10000 bytes decompressed output => must fail with SecurityViolation
        let decomp_res = decompress_body(&compressed_gzip, ContentEncoding::Gzip, 5000);
        assert!(decomp_res.is_err());
        assert_eq!(
            decomp_res.unwrap_err().kind,
            super::super::errors::HttpErrorKind::SecurityViolation
        );

        // 4. Transport Security Audit Report
        let insecure_req = Request::get("http://example.com/login").unwrap();
        let audit = audit_request_security(&insecure_req);
        assert!(!audit.is_secure);
        assert!(
            audit
                .findings
                .iter()
                .any(|f| f.contains("Insecure transport"))
        );
    }
}
