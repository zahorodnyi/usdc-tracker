# --- build stage ---
FROM rust:1.90.0-alpine AS build
WORKDIR /app

# встановлюємо інструменти для білду
RUN apk add --no-cache clang lld musl-dev git


COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps ./apps


RUN cargo build --locked --release --package tracker


RUN cp target/release/tracker /bin/server


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
