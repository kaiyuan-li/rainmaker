# Stage 1: Build the binary 
FROM rust:1.65.0 AS chef
WORKDIR rainmaker
RUN apt-get update && apt-get install -y cmake clang
COPY ./ ./
RUN cargo build --release \
    --bin as_okex

# Stage 2: Copy the bin to a slim debian image, the final size of image is <100MB
FROM debian:bullseye-slim AS trader
COPY --from=chef /rainmaker/target/release/as_okex /usr/local/bin

## Docker build command 
# docker build -t rainmaker_okx:latest .
## Docker run using an external json config
# docker run -v /home/zhu/proj/rainmaker:/app/data -it rainmaker_okx:latest /usr/local/bin/as_okex /app/data/okex_config.json

