//! Multi-Threaded Parallel Chunk Compression and Decompression.

use super::codecs::{CodecType, compress_bytes, decompress_bytes};
use std::sync::mpsc;
use std::thread;

pub fn parallel_compress(
    input: &[u8],
    codec: CodecType,
    chunk_size: usize,
    level: i32,
    num_threads: usize,
) -> Result<Vec<u8>, String> {
    let actual_chunk_sz = if chunk_size == 0 {
        128 * 1024
    } else {
        chunk_size
    };
    let threads = if num_threads == 0 {
        num_cpus::get().max(1)
    } else {
        num_threads
    };

    let chunks: Vec<Vec<u8>> = input.chunks(actual_chunk_sz).map(|c| c.to_vec()).collect();
    if chunks.is_empty() {
        return Ok(Vec::new());
    }

    let mut result_chunks: Vec<Option<Vec<u8>>> = vec![None; chunks.len()];
    let (tx, rx) = mpsc::channel();

    let mut chunk_indices: Vec<(usize, Vec<u8>)> = chunks.into_iter().enumerate().collect();
    let total = chunk_indices.len();
    let per_worker = (total + threads - 1) / threads;

    let mut handles = Vec::new();
    for _ in 0..threads {
        if chunk_indices.is_empty() {
            break;
        }
        let batch_size = per_worker.min(chunk_indices.len());
        let batch: Vec<(usize, Vec<u8>)> = chunk_indices.drain(..batch_size).collect();
        let tx = tx.clone();

        let handle = thread::spawn(move || {
            for (idx, block) in batch {
                let comp_res = compress_bytes(codec, &block, level);
                let _ = tx.send((idx, comp_res));
            }
        });
        handles.push(handle);
    }
    drop(tx);

    for _ in 0..total {
        match rx.recv() {
            Ok((idx, Ok(comp))) => {
                result_chunks[idx] = Some(comp);
            }
            Ok((_, Err(e))) => {
                return Err(format!("Parallel compression error: {}", e));
            }
            Err(e) => {
                return Err(format!("Parallel worker thread disconnected: {}", e));
            }
        }
    }

    for handle in handles {
        let _ = handle.join();
    }

    let mut output = Vec::new();
    // Prepend total chunk count
    output.extend_from_slice(&(total as u32).to_le_bytes());

    for comp_opt in result_chunks {
        let comp =
            comp_opt.ok_or_else(|| "Missing parallel compression result chunk".to_string())?;
        output.extend_from_slice(&(comp.len() as u32).to_le_bytes());
        output.extend_from_slice(&comp);
    }

    Ok(output)
}

pub fn parallel_decompress(
    input: &[u8],
    codec: CodecType,
    max_output_size: Option<usize>,
    num_threads: usize,
) -> Result<Vec<u8>, String> {
    if input.len() < 4 {
        return Err("Invalid parallel compressed payload: missing header".to_string());
    }

    let chunk_count = u32::from_le_bytes(input[..4].try_into().unwrap()) as usize;
    let threads = if num_threads == 0 {
        num_cpus::get().max(1)
    } else {
        num_threads
    };

    let mut pos = 4;
    let mut compressed_blocks = Vec::with_capacity(chunk_count);

    for _ in 0..chunk_count {
        if pos + 4 > input.len() {
            return Err("Truncated parallel compressed payload header".to_string());
        }
        let block_len = u32::from_le_bytes(input[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        if pos + block_len > input.len() {
            return Err("Truncated parallel compressed payload block".to_string());
        }
        compressed_blocks.push(input[pos..pos + block_len].to_vec());
        pos += block_len;
    }

    let mut decompressed_chunks: Vec<Option<Vec<u8>>> = vec![None; chunk_count];
    let (tx, rx) = mpsc::channel();

    let mut indexed_blocks: Vec<(usize, Vec<u8>)> =
        compressed_blocks.into_iter().enumerate().collect();
    let per_worker = (chunk_count + threads - 1) / threads;

    let mut handles = Vec::new();
    for _ in 0..threads {
        if indexed_blocks.is_empty() {
            break;
        }
        let batch_size = per_worker.min(indexed_blocks.len());
        let batch: Vec<(usize, Vec<u8>)> = indexed_blocks.drain(..batch_size).collect();
        let tx = tx.clone();

        let handle = thread::spawn(move || {
            for (idx, block) in batch {
                let decomp_res = decompress_bytes(codec, &block, max_output_size);
                let _ = tx.send((idx, decomp_res));
            }
        });
        handles.push(handle);
    }
    drop(tx);

    for _ in 0..chunk_count {
        match rx.recv() {
            Ok((idx, Ok(decomp))) => {
                decompressed_chunks[idx] = Some(decomp);
            }
            Ok((_, Err(e))) => {
                return Err(format!("Parallel decompression error: {}", e));
            }
            Err(e) => {
                return Err(format!("Parallel worker thread disconnected: {}", e));
            }
        }
    }

    for handle in handles {
        let _ = handle.join();
    }

    let mut output = Vec::new();
    for decomp_opt in decompressed_chunks {
        let decomp =
            decomp_opt.ok_or_else(|| "Missing parallel decompression result chunk".to_string())?;
        if let Some(max_sz) = max_output_size {
            if output.len() + decomp.len() > max_sz {
                return Err(format!(
                    "Decompression bomb limit exceeded: max {} bytes",
                    max_sz
                ));
            }
        }
        output.extend_from_slice(&decomp);
    }

    Ok(output)
}
