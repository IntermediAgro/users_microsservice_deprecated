FROM rust:1.71-slim as build

WORKDIR /app

COPY . .

RUN cargo build --release

FROM rust:1.71-slim

WORKDIR /usr/local/bin

COPY --from=build /app/target/release/microsservice .

EXPOSE 8080

CMD ["./microsservice"]
