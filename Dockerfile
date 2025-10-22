ARG RUST_VERSION=1.90.0
ARG APP_NAME=tracker

FROM rust:${RUST_VERSION}-alpine AS build
ARG APP_NAME
WORKDIR /app

RUN apk add --no-cache clang lld musl-dev git

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps ./apps

RUN cargo build --locked --release --package ${APP_NAME}

RUN cp target/release/${APP_NAME} /bin/server

FROM alpine:3.18 AS final
RUN apk add --no-cache ca-certificates

ARG UID=10001
RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "${UID}" \
    appuser
USER appuser
WORKDIR /app

COPY --from=build /bin/server /bin/
EXPOSE 8080
CMD ["/bin/server"]
