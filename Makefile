.PHONY: run dev test bundle install icon

run:
	cargo run

# Rebuild + relaunch on save (throwaway data in /tmp/workdesk-dev). Needs `brew install bacon`.
dev:
	bacon dev

test:
	cargo test

bundle:
	./scripts/bundle.sh

# Build + install/update /Applications/Workdesk.app and relaunch it.
install:
	./scripts/install.sh

icon:
	./scripts/make_icon.sh
