use bytes::{Bytes, BytesMut};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt;
use std::sync::{Arc, Mutex};

pub const MESSAGE_SIZE: usize = 1024;

pub type State = Arc<Mutex<HashMap<String, String>>>;

/// RedisValue is the canonical type for values flowing
/// through the system. Inputs are converted into RedisValues,
/// and outputs are converted into RedisValues.
#[derive(PartialEq, Clone)]
pub enum RedisValue {
	String(Bytes),
	Error(Bytes),
	Int(i64),
	Array(Vec<RedisValue>),
	NullArray,
	NullBulkString,
	ErrorMsg(Vec<u8>),
}

impl TryFrom<RedisValue> for Bytes {
	type Error = ReturnError;

	fn try_from(r: RedisValue) -> Result<Bytes, Self::Error> {
		match r {
			RedisValue::String(s) => Ok(s),
			_ => Err(ReturnError::UnknownType),
		}
	}
}

/// Fundamental struct for viewing byte slices
///
/// Used for zero-copy redis values.
pub struct BufSplit(pub usize, pub usize);

impl BufSplit {
	/// Get a lifetime appropriate slice of the underlying buffer.
	///
	/// Constant time.
	pub fn as_slice<'a>(&self, buf: &'a BytesMut) -> &'a [u8] {
		&buf[self.0..self.1]
	}

	/// Get a Bytes object representing the appropriate slice
	/// of bytes.
	///
	/// Constant time.
	pub fn as_bytes(&self, buf: &Bytes) -> Bytes {
		buf.slice(self.0..self.1)
	}
}

/// BufSplit based equivalent to our output type RedisValueRef
#[allow(dead_code)]
pub enum RedisBufSplit {
	String(BufSplit),
	Error(BufSplit),
	Int(i64),
	Array(Vec<RedisBufSplit>),
	NullArray,
	NullBulkString,
}

impl RedisBufSplit {
	pub fn redis_value(self, buf: &Bytes) -> RedisValue {
		match self {
			RedisBufSplit::String(bfs) => RedisValue::String(bfs.as_bytes(buf)),
			RedisBufSplit::Int(i) => RedisValue::Int(i),
			RedisBufSplit::Array(buf_values) => RedisValue::Array(
				buf_values
					.into_iter()
					.map(|bfs| bfs.redis_value(buf))
					.collect(),
			),
			RedisBufSplit::NullArray => RedisValue::NullArray,
			RedisBufSplit::NullBulkString => RedisValue::NullBulkString,
			RedisBufSplit::Error(bfs) => RedisValue::Error(bfs.as_bytes(buf)),
		}
	}
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum RESPError {
	UnexpectedEnd,
	UnknownStartingByte,
	IntParseFailure,
	NullBulkString,
	BadBulkStringSize,
	BadArraySize(usize),
}

#[derive(Debug)]
pub enum ReturnError {
	UnknownType,
}

impl fmt::Display for RESPError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			RESPError::UnexpectedEnd => write!(f, "UnexpectendEnd"),
			RESPError::UnknownStartingByte => write!(f, "UnknownStartingByte"),
			RESPError::IntParseFailure => write!(f, "IntParseFailure"),
			RESPError::NullBulkString => write!(f, "NullBulkString"),
			RESPError::BadBulkStringSize => write!(f, "BadBulkStringSize"),
			RESPError::BadArraySize(size) => write!(f, "BadBulkStringSize {}", size),
		}
	}
}

#[allow(dead_code)]
pub enum ReturnValue {
	Ok,
	StringRes(Bytes),
	MultiStringRes(Vec<Bytes>),
	Array(Vec<ReturnValue>),
	IntRes(i64),
	Nil,
}

impl ReturnValue {
	pub fn handle_string(b: Bytes) -> Result<ReturnValue, ReturnError> {
		let redis_string = String::from_utf8_lossy(&b);
		match redis_string.to_string().to_lowercase().as_str() {
			"echo" => Ok(ReturnValue::StringRes(write_simple_string(b))),
			"ping" => Ok(ReturnValue::StringRes(write_simple_string(Bytes::from(
				"PONG",
			)))),
			_ => Err(ReturnError::UnknownType),
		}
	}

	pub fn handle_array(a: Vec<RedisValue>, state: &mut State) -> Result<ReturnValue, ReturnError> {
		let head = Bytes::try_from(a[0].clone())?;
		let head_s = String::from_utf8_lossy(&head);
		match head_s.to_string().to_lowercase().as_str() {
			"echo" => {
				let response = Bytes::try_from(a[1].clone())?;
				Ok(ReturnValue::StringRes(write_bulk_string(response)))
			}
			"ping" => Ok(ReturnValue::StringRes(write_simple_string(Bytes::from(
				"PONG",
			)))),
			"set" => {
				let key = Bytes::try_from(a[1].clone())?;
				let key_s = String::from_utf8_lossy(&key).to_string();
				let value = Bytes::try_from(a[1].clone())?;
				let value_s = String::from_utf8_lossy(&value).to_string();

				match state.lock().unwrap().insert(key_s, value_s) {
					Some(old_value) => Ok(ReturnValue::StringRes(write_bulk_string(Bytes::from(
						old_value,
					)))),
					None => Ok(ReturnValue::Ok),
				}
			}
			"get" => {
				let key = Bytes::try_from(a[1].clone())?;
				let key_s = String::from_utf8_lossy(&key).to_string();

				match state.lock().unwrap().get(&key_s) {
					Some(value) => Ok(ReturnValue::StringRes(write_bulk_string(Bytes::from(
						value.to_string(),
					)))),
					None => Ok(ReturnValue::Nil),
				}
			}
			_ => Err(ReturnError::UnknownType),
		}
	}

	pub fn parse_redis_value(
		value: RedisValue,
		state: &mut State,
	) -> Result<ReturnValue, ReturnError> {
		match value {
			RedisValue::String(cmd) => ReturnValue::handle_string(cmd),
			RedisValue::Array(cmd) => ReturnValue::handle_array(cmd, state),
			_ => unimplemented!(),
		}
	}
}

pub fn write_simple_string(b: Bytes) -> Bytes {
	let s = String::from_utf8_lossy(&b);
	Bytes::from(format!("+{}\r\n", s))
}

pub fn write_bulk_string(b: Bytes) -> Bytes {
	let s = String::from_utf8_lossy(&b);
	let size = s.len();
	Bytes::from(format!("${}\r\n{}\r\n", size, s))
}

pub fn write_bulk_string_nil() -> Bytes {
	Bytes::from(format!("$-1\r\n"))
}
