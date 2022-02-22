# Copyless Redis
A basic implementation of the Redis protocol in Rust. This implementation uses a copyless design, where the receiving buffer is not copied, allowing for quick execution.

This implementation supports `PING`, `SET`, and `GET`.