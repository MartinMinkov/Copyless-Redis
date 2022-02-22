mod redis;
mod types;

use bytes::{Bytes, BytesMut};
use std::collections::HashMap;

use redis::*;
#[allow(unused_imports)]
use std::env;
use types::*;

#[allow(unused_imports)]
use std::fs;
#[allow(unused_imports)]
use std::io::{Error, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

fn handle_command(stream: &mut TcpStream, redis_value: RedisValue, state: &mut State) {
    match ReturnValue::parse_redis_value(redis_value, state) {
        Ok(ReturnValue::StringRes(s)) => {
            // let test = String::from_utf8_lossy(&s);
            // println!("return value {}", test);
            let _ = stream.write(&s);
        }
        Ok(ReturnValue::Ok) => {
            let ok_response = write_simple_string(Bytes::from("OK"));
            let _ = stream.write(&ok_response);
        }
        Ok(ReturnValue::Nil) => {
            let nil_response = write_bulk_string_nil();
            let _ = stream.write(&nil_response);
        }
        _ => {
            println!("Cannot find return value");
        }
    }
}

fn handle_message(stream: &mut TcpStream, buf: &mut BytesMut, state: &mut State) {
    println!("Received data: {}.", String::from_utf8_lossy(&buf));
    match parse(buf, 0) {
        Ok(result) => match result {
            Some((pos, value)) => {
                let data = buf.split_to(pos);
                let redis_value = value.redis_value(&data.freeze());
                handle_command(stream, redis_value, state)
            }
            None => {}
        },
        Err(e) => {
            println!("Error parsing: {}", e)
        }
    }
}

fn handle_client(mut stream: TcpStream, state: &mut State) -> Result<(), Error> {
    println!("Incoming connection from: {}", stream.peer_addr()?);
    let mut temp_buf = [0; MESSAGE_SIZE];

    loop {
        let bytes_read = stream.read(&mut temp_buf)?;
        let mut buf = BytesMut::from(&temp_buf[..]);
        handle_message(&mut stream, &mut buf, state);
        if bytes_read == 0 {
            return Ok(());
        }
    }
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:6379").unwrap();
    let state: State = Arc::new(Mutex::new(HashMap::new()));
    for stream in listener.incoming() {
        match stream {
            Err(e) => eprintln!("failed {}", e),
            Ok(stream) => {
                let mut state = Arc::clone(&state);
                thread::spawn(move || {
                    handle_client(stream, &mut state)
                        .unwrap_or_else(|error| eprintln!("failed {:?}", error));
                });
            }
        }
    }
}
