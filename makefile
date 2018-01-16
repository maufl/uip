.PHONY: target/release/uipd

target/release/uipd:
	cargo build --release --bin uipd

deploy: target/release/uipd
	ssh -t maufl "sudo systemctl stop uipd"
	scp target/release/uipd maufl:/usr/local/bin/uipd
	ssh -t maufl "sudo systemctl start uipd"
