use std::vec::Vec;

pub fn write_u32_leb(mut v: u32, out: &mut Vec<u8>) {
    loop {
        let mut b = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}

pub fn write_i32_leb(mut v: i32, out: &mut Vec<u8>) {
    loop {
        let byte = (v & 0x7F) as u8;
        v >>= 7;
        let sign_bit = (byte & 0x40) != 0;
        if (v == 0 && !sign_bit) || (v == -1 && sign_bit) {
            out.push(byte);
            break;
        } else {
            out.push(byte | 0x80);
        }
    }
}

pub fn encode_type_section(func_sigs: &[(Vec<u8>, Option<u8>)]) -> Vec<u8> {
    let mut payload = Vec::new();
    write_u32_leb(func_sigs.len() as u32, &mut payload);
    for (params, ret) in func_sigs {
        payload.push(0x60);
        write_u32_leb(params.len() as u32, &mut payload);
        for p in params {
            payload.push(*p);
        }
        match ret {
            Some(t) => {
                payload.push(0x01);
                payload.push(*t);
            }
            None => payload.push(0x00),
        }
    }
    let mut sec = Vec::new();
    sec.push(0x01);
    write_u32_leb(payload.len() as u32, &mut sec);
    sec.extend(payload);
    sec
}

pub fn encode_import_section() -> Vec<u8> {
    let mut payload = Vec::new();
    // Import count: print_f64, print_str, alloc, concat2, file_write_str
    write_u32_leb(5, &mut payload);
    // import env.print_f64 : type 0
    let module = b"env";
    let name = b"print_f64";
    write_u32_leb(module.len() as u32, &mut payload);
    payload.extend(module);
    write_u32_leb(name.len() as u32, &mut payload);
    payload.extend(name);
    payload.push(0x00);
    write_u32_leb(0, &mut payload);
    // import env.print_str : type 1
    let name2 = b"print_str";
    write_u32_leb(module.len() as u32, &mut payload);
    payload.extend(module);
    write_u32_leb(name2.len() as u32, &mut payload);
    payload.extend(name2);
    payload.push(0x00);
    write_u32_leb(1, &mut payload);
    // import env.alloc : type 3 (i32)->i32
    let name3 = b"alloc";
    write_u32_leb(module.len() as u32, &mut payload);
    payload.extend(module);
    write_u32_leb(name3.len() as u32, &mut payload);
    payload.extend(name3);
    payload.push(0x00);
    write_u32_leb(3, &mut payload);
    // import env.concat2 : type 4 (i32,i32,i32,i32)->i32
    let name4 = b"concat2";
    write_u32_leb(module.len() as u32, &mut payload);
    payload.extend(module);
    write_u32_leb(name4.len() as u32, &mut payload);
    payload.extend(name4);
    payload.push(0x00);
    write_u32_leb(4, &mut payload);
    // import env.file_write_str : type 6 (i32,i32,i32,i32)->()
    let name5 = b"file_write_str";
    write_u32_leb(module.len() as u32, &mut payload);
    payload.extend(module);
    write_u32_leb(name5.len() as u32, &mut payload);
    payload.extend(name5);
    payload.push(0x00);
    write_u32_leb(6, &mut payload);
    let mut sec = Vec::new();
    sec.push(0x02);
    write_u32_leb(payload.len() as u32, &mut sec);
    sec.extend(payload);
    sec
}

pub fn encode_function_section(func_types: &[u32]) -> Vec<u8> {
    let mut payload = Vec::new();
    write_u32_leb(func_types.len() as u32, &mut payload);
    for t in func_types {
        write_u32_leb(*t, &mut payload);
    }
    let mut sec = Vec::new();
    sec.push(0x03);
    write_u32_leb(payload.len() as u32, &mut sec);
    sec.extend(payload);
    sec
}

pub fn encode_export_section(func_index: u32, export_memory: bool) -> Vec<u8> {
    let mut payload = Vec::new();
    // Exports: main, malloc, [memory]
    let count = if export_memory { 3 } else { 2 };
    write_u32_leb(count, &mut payload);

    // export function "main"
    let name = b"main";
    write_u32_leb(name.len() as u32, &mut payload);
    payload.extend(name);
    payload.push(0x00); // func kind
    write_u32_leb(func_index, &mut payload);

    // export function "malloc" (aliases import #2 'alloc')
    let mname = b"malloc";
    write_u32_leb(mname.len() as u32, &mut payload);
    payload.extend(mname);
    payload.push(0x00); // func kind
    write_u32_leb(2, &mut payload); // Index 2 is 'alloc' import

    if export_memory {
        let mem_name = b"memory";
        write_u32_leb(mem_name.len() as u32, &mut payload);
        payload.extend(mem_name);
        payload.push(0x02); // memory kind
        write_u32_leb(0, &mut payload); // memory 0
    }
    let mut sec = Vec::new();
    sec.push(0x07);
    write_u32_leb(payload.len() as u32, &mut sec);
    sec.extend(payload);
    sec
}

pub fn encode_code_section(funcs: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
    let mut payload = Vec::new();
    write_u32_leb(funcs.len() as u32, &mut payload);
    for (local_i32_count, local_f64_count, body) in funcs {
        let mut func = Vec::new();
        let mut group_count = 0u32;
        let mut locals = Vec::new();
        if *local_i32_count > 0 {
            group_count += 1;
        }
        if *local_f64_count > 0 {
            group_count += 1;
        }
        if group_count == 0 {
            func.push(0x00);
        } else {
            write_u32_leb(group_count, &mut locals);
            if *local_i32_count > 0 {
                write_u32_leb(*local_i32_count, &mut locals);
                locals.push(0x7F);
            }
            if *local_f64_count > 0 {
                write_u32_leb(*local_f64_count, &mut locals);
                locals.push(0x7C);
            }
            func.extend(locals);
        }
        func.extend(body.iter());
        func.push(0x0B);
        let mut body_buf = Vec::new();
        write_u32_leb(func.len() as u32, &mut body_buf);
        body_buf.extend(func);
        payload.extend(body_buf);
    }
    let mut sec = Vec::new();
    sec.push(0x0A);
    write_u32_leb(payload.len() as u32, &mut sec);
    sec.extend(payload);
    sec
}

pub fn encode_memory_section(min_pages: u32) -> Vec<u8> {
    let mut payload = Vec::new();
    write_u32_leb(1, &mut payload); // one memory
    payload.push(0x00); // limits: min only
    write_u32_leb(min_pages, &mut payload);
    let mut sec = Vec::new();
    sec.push(0x05);
    write_u32_leb(payload.len() as u32, &mut sec);
    sec.extend(payload);
    sec
}

pub fn encode_data_section(segments: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut payload = Vec::new();
    write_u32_leb(segments.len() as u32, &mut payload);
    for (offset, bytes) in segments {
        write_u32_leb(0, &mut payload); // memory index 0
        // offset expr: i32.const <offset>; end
        payload.push(0x41);
        write_u32_leb(*offset, &mut payload);
        payload.push(0x0B);
        write_u32_leb(bytes.len() as u32, &mut payload);
        payload.extend(bytes);
    }
    let mut sec = Vec::new();
    sec.push(0x0B);
    write_u32_leb(payload.len() as u32, &mut sec);
    sec.extend(payload);
    sec
}
