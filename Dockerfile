FROM rust:1.50-alpine3.13 AS builder
WORKDIR /usr/src/
RUN apk add --no-cache musl-dev
RUN mkdir /usr/src/squo
COPY src /usr/src/squo/src
COPY Cargo.toml /usr/src/squo/
WORKDIR /usr/src/squo
RUN cargo install --path .

FROM alpine:3.13
COPY --from=builder /usr/local/cargo/bin/squo /usr/local/bin/squo
ENTRYPOINT ["/usr/local/bin/squo"]

