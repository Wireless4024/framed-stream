# Framed stream

This library allow to read stream and preserve end of buffer.

## Example

```rust
fn test() {
	use framed_stream::Frame;
	// 8 is capacity, 2 is preserve size
	let mut bytes = Frame::new(8, 2);
	assert_eq!(bytes.extend_from_slice(b"Hello"), 5);
	// use `.deref` to access buffer
	// if your function accept &[u8] or impl AsRef<[u8]> 
	//   you can pass `&bytes` instead
	assert_eq!(bytes.deref(), b"Hello");
	// "lo" is preserved and appear in next buffer
	bytes.consume();
	bytes.extend_from_slice(b"west");
	// "lo" is preserved during consume
	assert_eq!(bytes.deref(), b"lowest");
}
```