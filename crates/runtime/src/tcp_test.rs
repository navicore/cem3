//! Minimal May TCP test to verify the library works
//!
//! This test uses May's networking directly from Rust to verify
//! the issue isn't with May itself, but with how we're using it.

#[cfg(test)]
mod tests {
    use may::coroutine;
    use may::net::{TcpListener, TcpStream};
    use std::io::{Read, Write};

    #[test]
    fn test_may_tcp_basic() {
        unsafe {
            // Initialize May (same as scheduler_init)
            may::config().set_stack_size(0x100000); // 1MB stack

            // Spawn server coroutine
            let server = coroutine::spawn(|| {
                let listener = TcpListener::bind("127.0.0.1:19999").unwrap();
                let (mut stream, _) = listener.accept().unwrap();

                // Read request
                let mut buffer = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    match stream.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => buffer.extend_from_slice(&chunk[..n]),
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(e) => panic!("Server read failed: {}", e),
                    }
                    if buffer.len() >= 4 && &buffer[buffer.len() - 4..] == b"\r\n\r\n" {
                        break;
                    }
                }

                // Write response
                let response = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHello";
                stream.write_all(response).unwrap();
                stream.flush().unwrap();
            });

            // Give server time to start
            std::thread::sleep(std::time::Duration::from_millis(100));

            // Spawn client coroutine
            let client = coroutine::spawn(|| {
                let mut stream = TcpStream::connect("127.0.0.1:19999").unwrap();

                // Send request
                stream.write_all(b"GET / HTTP/1.1\r\n\r\n").unwrap();
                stream.flush().unwrap();

                // Read response
                let mut buffer = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    match stream.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => buffer.extend_from_slice(&chunk[..n]),
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(e) => panic!("Client read failed: {}", e),
                    }
                    if buffer.len() >= 5 {
                        break;
                    }
                }

                let response = String::from_utf8(buffer).unwrap();
                assert!(response.contains("200 OK"));
                assert!(response.contains("Hello"));
            });

            // Wait for both to complete
            server.join().unwrap();
            client.join().unwrap();
        }
    }
}
