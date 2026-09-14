//! Safe CLI cryptography utility handler (`adl crypto`).

use rand::RngCore;
use sha2::{Digest, Sha256};
use std::fs;

pub fn execute_crypto_cli(args: &[String]) {
    if args.is_empty() {
        println!("AdeshLang Safe Cryptography CLI Utility");
        println!("Usage:");
        println!("  adl crypto sha256 <filepath>          Compute SHA-256 hash of a file");
        println!("  adl crypto random <num_bytes>         Generate secure random hex bytes");
        println!("  adl crypto inspect-cert <cert.pem>    Inspect X.509 Certificate metadata");
        return;
    }

    let subcommand = args[0].as_str();
    match subcommand {
        "sha256" | "hash" => {
            if args.len() < 2 {
                eprintln!(
                    "Error: Please specify a file path. Example: `adl crypto sha256 file.txt`"
                );
                return;
            }
            let file_path = &args[1];
            match fs::read(file_path) {
                Ok(data) => {
                    let digest = Sha256::digest(&data);
                    println!("SHA-256 ({}): {}", file_path, hex::encode(digest));
                }
                Err(e) => eprintln!("Failed to read file '{}': {}", file_path, e),
            }
        }
        "random" => {
            let count = args
                .get(1)
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(32);
            let mut bytes = vec![0u8; count];
            rand::thread_rng().fill_bytes(&mut bytes);
            println!(
                "Secure Random ({} bytes hex): {}",
                count,
                hex::encode(bytes)
            );
        }
        "inspect-cert" => {
            if args.len() < 2 {
                eprintln!(
                    "Error: Please specify a certificate file. Example: `adl crypto inspect-cert cert.pem`"
                );
                return;
            }
            let file_path = &args[1];
            match fs::read_to_string(file_path) {
                Ok(pem_str) => {
                    match crate::runtime::stdlib_src::crypto::pem_der_cert::parse_x509_pem(&pem_str)
                    {
                        Ok(cert) => {
                            println!("X.509 Certificate Summary:");
                            println!("  Subject         : {}", cert.subject);
                            println!("  Issuer          : {}", cert.issuer);
                            println!("  Serial (Hex)    : {}", cert.serial_hex);
                            println!("  Valid From      : {}", cert.not_before);
                            println!("  Valid To        : {}", cert.not_after);
                            println!("  SHA-256 Fingerprint: {}", cert.fingerprint_sha256);
                            println!("  Is CA           : {}", cert.is_ca);
                        }
                        Err(e) => eprintln!("Failed to parse certificate: {}", e),
                    }
                }
                Err(e) => eprintln!("Failed to read certificate file '{}': {}", file_path, e),
            }
        }
        _ => {
            eprintln!(
                "Unknown crypto command: '{}'. Run `adl crypto` for help.",
                subcommand
            );
        }
    }
}
