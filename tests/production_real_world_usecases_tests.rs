//! Production-grade Real-world Use Case Tests for AdeshLang
//!
//! Covers:
//! - Financial Transaction Processing & Ledger Calculations
//! - Pipeline processing (Map, Filter, Reduce)
//! - State Machine (Parser / Tokenizer state)
//! - Complex Data Structures & Algorithms (Binary Search, Quick Sort simulation)

use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;

fn run_test_code(src: &str) -> Result<(), String> {
    let src_owned = src.to_string();
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut loader = ModuleLoader::new(Path::new("."));
            let mut interp = Interpreter::new();
            interp
                .run_module(&src_owned, &mut loader, None)
                .map_err(|e| e.to_string())
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn test_financial_ledger_pipeline() {
    let code = r#"
        struct Transaction {
            id: u64,
            amount: f64,
            is_credit: bool,
            category: string,
        }

        class Ledger {
            fn init() {
                this.transactions = [];
            }

            fn record(tx: Transaction) {
                this.transactions.append(tx);
            }

            fn calculate_balance() -> f64 {
                let balance = 0.0;
                for tx in this.transactions {
                    if (tx.is_credit) {
                        balance = balance + tx.amount;
                    } else {
                        balance = balance - tx.amount;
                    }
                }
                return balance;
            }

            fn count_category(cat: string) -> i32 {
                let count = 0;
                for tx in this.transactions {
                    if (tx.category == cat) {
                        count = count + 1;
                    }
                }
                return count;
            }
        }

        let ledger = new Ledger();
        ledger.record(Transaction { id: 1, amount: 5000.0, is_credit: true, category: "SALARY" });
        ledger.record(Transaction { id: 2, amount: 1200.0, is_credit: false, category: "RENT" });
        ledger.record(Transaction { id: 3, amount: 350.0, is_credit: false, category: "GROCERY" });
        ledger.record(Transaction { id: 4, amount: 150.0, is_credit: false, category: "GROCERY" });

        assert_eq(ledger.calculate_balance(), 3300.0);
        assert_eq(ledger.count_category("GROCERY"), 2);
    "#;
    let res = run_test_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_state_machine_protocol_parser() {
    let code = r#"
        class ProtocolParser {
            fn init() {
                this.state = "HEADER";
                this.payload_bytes = [];
            }

            fn feed_byte(b: u8) -> string {
                if (this.state == "HEADER") {
                    if (b == 0xAA) {
                        this.state = "PAYLOAD";
                        return "FOUND_HEADER";
                    }
                    return "WAITING_HEADER";
                } else if (this.state == "PAYLOAD") {
                    if (b == 0xFF) {
                        this.state = "COMPLETED";
                        return "PACKET_COMPLETE";
                    }
                    this.payload_bytes.append(b);
                    return "CONSUMED_BYTE";
                }
                return "ALREADY_COMPLETED";
            }

            fn payload_length() -> i32 {
                return this.payload_bytes.len();
            }
        }

        let parser = new ProtocolParser();
        assert_eq(parser.feed_byte(0x00), "WAITING_HEADER");
        assert_eq(parser.feed_byte(0xAA), "FOUND_HEADER");
        assert_eq(parser.feed_byte(0x10), "CONSUMED_BYTE");
        assert_eq(parser.feed_byte(0x20), "CONSUMED_BYTE");
        assert_eq(parser.feed_byte(0xFF), "PACKET_COMPLETE");

        assert_eq(parser.payload_length(), 2);
    "#;
    let res = run_test_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_binary_search_algorithm() {
    let code = r#"
        fn binary_search(arr: [i32], target: i32) -> i32 {
            let low = 0;
            let high = arr.len() - 1;

            while (low <= high) {
                let mid = (low + high) ~/ 2;
                if (arr[mid] == target) {
                    return mid;
                }
                if (arr[mid] < target) {
                    low = mid + 1;
                } else {
                    high = mid - 1;
                }
            }
            return -1;
        }

        let primes = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31];
        assert_eq(binary_search(primes, 13), 5);
        assert_eq(binary_search(primes, 2), 0);
        assert_eq(binary_search(primes, 31), 10);
        assert_eq(binary_search(primes, 100), -1);
    "#;
    let res = run_test_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}
