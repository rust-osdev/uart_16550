CARGO ?= cargo
TARGET := x86_64-unknown-uefi
TARGET_DIR := ../target/real-hw-test
PROFILE := release
BINARY := $(TARGET_DIR)/$(TARGET)/$(PROFILE)/uart-16550-real-hw-test.efi
ARTIFACT := build/BOOTX64.EFI

.PHONY: all artifact check clean

all: artifact

artifact: $(ARTIFACT)

$(ARTIFACT):
	CARGO_TARGET_DIR=$(TARGET_DIR) $(CARGO) build --locked --target $(TARGET) --release
	mkdir -p $(dir $(ARTIFACT))
	cp $(BINARY) $(ARTIFACT)

check:
	$(CARGO) fmt --check
	CARGO_TARGET_DIR=$(TARGET_DIR) $(CARGO) clippy --locked --target $(TARGET) --release -- -D warnings
	$(MAKE) artifact

clean:
	$(CARGO) clean --target-dir $(TARGET_DIR)
	rm -rf build

