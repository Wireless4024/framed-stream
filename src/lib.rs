use std::io;
use std::ops::Deref;

use bytes::BytesMut;
#[cfg(feature = "monoio")]
use monoio::io::AsyncReadRent;
#[cfg(feature = "tokio")]
use tokio::io::{AsyncRead, AsyncReadExt};

/// Buffer frame allow to read new data and retain some part of buffer
pub struct Frame {
	reserved: usize,
	capacity: usize,
	buf: BytesMut,
	#[cfg(feature = "monoio")]
	spare_buf: Option<BytesMut>,
	#[cfg(feature = "read_monoio_file")]
	offset: u64,
	#[cfg(feature = "monoio")]
	_marker: std::marker::PhantomPinned,
}

impl Frame {
	pub fn new(capacity: usize, reserved: usize) -> Self {
		if reserved < (capacity >> 2) { panic!("Please use larger buffer size") }
		Self {
			buf: BytesMut::with_capacity(capacity),
			capacity,
			reserved,
			#[cfg(feature = "monoio")]
			spare_buf: Some(BytesMut::with_capacity(0)),
			#[cfg(feature = "read_monoio_file")]
			offset: 0,
			#[cfg(feature = "monoio")]
			_marker: std::marker::PhantomPinned,
		}
	}

	#[inline]
	fn reserve(&mut self) {
		self.buf.reserve(self.capacity - self.reserved)
	}

	pub fn extend_from_slice(&mut self, slice: &[u8]) -> usize {
		self.reserve();
		let need = (self.buf.capacity() - self.buf.len()).min(slice.len());
		self.buf.extend_from_slice(&slice[..need]);
		need
	}

	#[cfg(feature = "tokio")]
	pub async fn read_tokio<R: AsyncRead + Unpin>(&mut self, reader: &mut R) -> io::Result<bool> {
		self.reserve();
		loop {
			match reader.read_buf(&mut self.buf).await {
				Ok(0) => {
					break Ok(false);
				}
				Ok(n) => {
					if n < (self.reserved << 1) {
						continue;
					} else {
						break Ok(true);
					}
				}
				Err(err) => {
					break Err(err);
				}
			}
		}
	}

	#[cfg(feature = "monoio")]
	pub async fn read_monoio<R: AsyncReadRent + Unpin>(&mut self, reader: &mut R) -> io::Result<bool> {
		self.reserve();
		let mut spare = self.spare_buf.take().unwrap_or_default();
		std::mem::swap(&mut spare, &mut self.buf);
		loop {
			let (res, buf) = reader.read(spare).await;
			spare = buf;
			std::mem::swap(&mut spare, &mut self.buf);
			match res {
				Ok(0) => { break Ok(false); }
				Ok(n) => {
					if n < (self.reserved << 1) {
						continue;
					} else {
						break Ok(true);
					}
				}
				Err(err) => break Err(err)
			}
		}
	}

	#[cfg(feature = "read_monoio_file")]
	pub async fn read_monoio_file(&mut self, reader: &monoio::fs::File) -> io::Result<bool> {
		self.reserve();
		let mut spare = self.spare_buf.take().unwrap_or_default();
		std::mem::swap(&mut spare, &mut self.buf);
		loop {
			let buf = spare.split_off(spare.len());
			let (res, buf) = reader.read_at(buf, self.offset).await;
			spare.unsplit(buf);
			std::mem::swap(&mut spare, &mut self.buf);
			match res {
				Ok(0) => { break Ok(false); }
				Ok(n) => {
					self.offset += n as u64;
					if n < (self.reserved << 1) {
						continue;
					} else {
						break Ok(true);
					}
				}
				Err(err) => break Err(err)
			}
		}
	}

	pub fn consume(&mut self) -> BytesMut {
		self.buf.split_to(self.buf.len() - self.reserved)
	}
}

impl Deref for Frame {
	type Target = [u8];

	fn deref(&self) -> &Self::Target { &self.buf }
}

#[cfg(test)]
mod tests {
	use std::ops::Deref;

	use crate::Frame;

	#[test]
	fn test_bytes() {
		let mut bytes = Frame::new(8, 2);
		let ptr = bytes.buf.as_ptr() as usize;
		assert_eq!(bytes.extend_from_slice(b"Hello"), 5);
		assert_eq!(bytes.deref(), b"Hello");
		bytes.consume();
		bytes.extend_from_slice(b"west");
		let ptr2 = bytes.buf.as_ptr() as usize;
		assert_eq!(bytes.deref(), b"lowest");
		// check that no reallocation caused
		assert_eq!(ptr, ptr2);
	}

	#[cfg(feature = "tokio")]
	#[tokio::test]
	async fn test_bytes_tokio() {
		use tokio::fs::File;
		let mut bytes = Frame::new(8, 2);
		let mut file = File::open(".gitignore").await.unwrap();
		let ptr = bytes.buf.as_ptr() as usize;
		if bytes.read_tokio(&mut file).await.is_err() {
			panic!("Error during read file");
		}
		assert_eq!(bytes.deref(), b"/target\n");
		bytes.consume();
		if bytes.read_tokio(&mut file).await.is_err() {
			panic!("Error during read file");
		}
		let ptr2 = bytes.buf.as_ptr() as usize;
		assert_eq!(bytes.deref(), b"t\n/Cargo");
		// check that no reallocation caused
		assert_eq!(ptr, ptr2);
	}

	#[test]
	#[cfg(feature = "read_monoio_file")]
	fn test_bytes_monoio() {
		use monoio::FusionDriver;
		use monoio::fs::File;
		monoio::RuntimeBuilder::<FusionDriver>::new()
			.enable_all()
			.build()
			.unwrap()
			.block_on(async {
				let mut bytes = Frame::new(8, 2);
				let file = File::open(".gitignore").await.unwrap();
				let ptr = bytes.buf.as_ptr() as usize;
				if bytes.read_monoio_file(&file).await.is_err() {
					panic!("Error during read file");
				}
				assert_eq!(bytes.deref(), b"/target\n");
				bytes.consume();
				if bytes.read_monoio_file(&file).await.is_err() {
					panic!("Error during read file");
				}
				let ptr2 = bytes.buf.as_ptr() as usize;
				assert_eq!(bytes.deref(), b"t\n/Cargo");
				// check that no reallocation caused
				assert_eq!(ptr, ptr2);
			});
	}
}