FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
	cmake curl g++ libclang-dev libssl-dev

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y

#RUN echo 'source $HOME/.cargo/env' >> $HOME/.bashrc
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /usr/src/srt-rust-server

# build dependencies with dummy main file
COPY ./Cargo.toml ./
RUN mkdir src/ && echo "fn main() {}" > src/main.rs
RUN cargo build --release

COPY ./ ./
RUN cargo build --release

#CMD bash
CMD cargo run --release
