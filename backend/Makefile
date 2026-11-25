# Run from the backend directory. Settings are read from .env (see .env.example).

.PHONY: build migrate

build:
	cargo build --release

migrate:
	cargo run --bin console -- migrate
