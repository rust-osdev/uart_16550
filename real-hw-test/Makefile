CARGO ?= cargo
QEMU ?= qemu-system-x86_64
QEMU_ACCEL ?= kvm
TARGET := x86_64-unknown-uefi
TARGET_DIR := ../target/real-hw-test
PROFILE := release
BINARY := $(TARGET_DIR)/$(TARGET)/$(PROFILE)/uart-16550-real-hw-test.efi
ARTIFACT := build/BOOTX64.EFI
BUILD := CARGO_TARGET_DIR=$(TARGET_DIR) $(CARGO) build --locked
CLIPPY := CARGO_TARGET_DIR=$(TARGET_DIR) $(CARGO) clippy --locked

.PHONY: all artifact check qemu qemu-tcg clean

all: artifact

artifact: $(ARTIFACT)

$(ARTIFACT):
	$(BUILD) --target $(TARGET) --release
	mkdir -p $(dir $(ARTIFACT))
	cp $(BINARY) $(ARTIFACT)

check:
	$(CARGO) fmt --check
	$(CLIPPY) --target $(TARGET) --release -- -D warnings
	$(MAKE) artifact

qemu: artifact
	QEMU="$(QEMU)" QEMU_ACCEL="$(QEMU_ACCEL)" OVMF="$(OVMF)" \
		./scripts/run-qemu.sh $(QEMU_ARGS)

qemu-tcg:
	$(MAKE) qemu QEMU_ACCEL=tcg

clean:
	$(CARGO) clean --target-dir $(TARGET_DIR)
	rm -rf build
