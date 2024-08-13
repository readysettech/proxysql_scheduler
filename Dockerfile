# Use the latest official Rust
FROM rust:latest

# install dependencies
RUN cargo install cargo-generate-rpm cargo-deb
