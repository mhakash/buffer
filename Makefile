.PHONY: run dev test bundle install icon

run:
	cargo run

# Rebuild + relaunch on save (throwaway data in /tmp/buffer-dev). Needs `brew install bacon`.
dev:
	bacon dev

test:
	cargo test

bundle:
	./scripts/bundle.sh

# Build + install/update ~/Applications/Buffer.app and relaunch it.
install:
	./scripts/install.sh

icon:
	./scripts/make_icon.sh
