# 1. This tells docker to use the Rust official image
FROM rust:1.67-alpine

WORKDIR /rainmaker
# 2. Copy the files in your machine to the Docker image
COPY ./ ./

# Dependencies to run cargo build
RUN apk add g++ make cmake

# Build your program for release
RUN cargo build --release