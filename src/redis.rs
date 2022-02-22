use super::types::*;
use bytes::BytesMut;
use std::str;

type RedisResult = Result<Option<(usize, RedisBufSplit)>, RESPError>;

pub fn word(buf: &BytesMut, pos: usize) -> Result<Option<(usize, BufSplit)>, RESPError> {
	if buf.len() <= pos {
		return Ok(None);
	}

	// Find the position of '\r' in the buffer
	let mut end = pos;
	while buf[end] != b'\r' {
		if buf.len() <= end + 1 {
			return Ok(None);
		} else {
			end += 1;
		}
	}

	if end + 1 < buf.len() {
		return Ok(Some((end + 2, BufSplit(pos, end))));
	} else {
		Ok(None)
	}
}

pub fn integer(buf: &BytesMut, pos: usize) -> Result<Option<(usize, i64)>, RESPError> {
	match word(buf, pos)? {
		Some((pos, word)) => {
			let s = str::from_utf8(word.as_slice(buf)).map_err(|_| RESPError::IntParseFailure)?;
			let i = s.parse::<i64>().map_err(|_| RESPError::IntParseFailure)?;
			Ok(Some((pos, i)))
		}
		None => Ok(None),
	}
}

pub fn bulk_string(buf: &BytesMut, pos: usize) -> RedisResult {
	match integer(buf, pos)? {
		Some((_pos, -1)) => Err(RESPError::NullBulkString),
		Some((pos, bulk_size)) => {
			if bulk_size < 0 {
				return Err(RESPError::BadBulkStringSize);
			} else {
				let total_size = pos + bulk_size as usize;
				if buf.len() < total_size + 2 {
					return Ok(None);
				} else {
					let bulk_string = RedisBufSplit::String(BufSplit(pos, total_size));
					return Ok(Some((total_size + 2, bulk_string)));
				}
			}
		}
		None => Ok(None),
	}
}

pub fn array(buf: &BytesMut, pos: usize) -> RedisResult {
	match integer(buf, pos)? {
		None => Ok(None),
		Some((pos, -1)) => Ok(Some((pos, RedisBufSplit::NullArray))),
		Some((pos, num_elements)) if num_elements >= 0 => {
			let mut values = Vec::with_capacity(num_elements as usize);
			let mut curr_pos = pos;
			for _ in 0..num_elements {
				match parse(buf, curr_pos)? {
					Some((new_pos, redis_value)) => {
						curr_pos = new_pos;
						values.push(redis_value)
					}
					None => return Ok(None),
				}
			}
			Ok(Some((curr_pos, RedisBufSplit::Array(values))))
		}
		Some((_, bad_elements_size)) => Err(RESPError::BadArraySize(bad_elements_size as usize)),
	}
}

pub fn simple_string(buf: &BytesMut, pos: usize) -> RedisResult {
	match word(buf, pos)? {
		Some((pos, word)) => Ok(Some((pos, RedisBufSplit::String(word)))),
		None => Ok(None),
	}
}

pub fn resp_int(buf: &BytesMut, pos: usize) -> RedisResult {
	match integer(buf, pos)? {
		Some((pos, number)) => Ok(Some((pos, RedisBufSplit::Int(number)))),
		None => Ok(None),
	}
}

pub fn parse(buf: &BytesMut, pos: usize) -> RedisResult {
	if buf.is_empty() {
		return Ok(None);
	}

	//println!("PARSING DATA: {}", String::from_utf8_lossy(&buf));
	match buf[pos] {
		b'+' => simple_string(buf, pos + 1),
		b'-' => unimplemented!(),
		b'$' => bulk_string(buf, pos + 1),
		b':' => resp_int(buf, pos + 1),
		b'*' => array(buf, pos + 1),
		_ => Err(RESPError::UnknownStartingByte),
	}
}
