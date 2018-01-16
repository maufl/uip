.PHONY: target/release/uipd

target/release/uipd:
	cargo build --release --bin uipd

deploy: target/release/uipd
	scp target/release/uipd maufl:/usr/local/bin/uipd
