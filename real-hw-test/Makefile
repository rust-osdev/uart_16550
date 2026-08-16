CARGO ?= cargo
ARCH ?= x86_64
ARCHS := x86_64 aarch64

ifeq ($(ARCH),x86_64)
TARGET := x86_64-unknown-uefi
BOOT_NAME := BOOTX64.EFI
QEMU ?= qemu-system-x86_64
QEMU_ACCEL ?= kvm
else ifeq ($(ARCH),aarch64)
TARGET := aarch64-unknown-uefi
BOOT_NAME := BOOTAA64.EFI
QEMU ?= qemu-system-aarch64
# The development host is typically x86_64, so hardware acceleration is not
# assumed for the aarch64 guest.
QEMU_ACCEL ?= tcg
else
$(error unsupported ARCH '$(ARCH)'; supported: x86_64, aarch64)
endif

TARGET_DIR := ../target/real-hw-test
PROFILE := release
BINARY := $(TARGET_DIR)/$(TARGET)/$(PROFILE)/uart-16550-real-hw-test.efi
ARTIFACT := build/$(BOOT_NAME)
BUILD := CARGO_TARGET_DIR=$(TARGET_DIR) $(CARGO) build
CLIPPY := CARGO_TARGET_DIR=$(TARGET_DIR) $(CARGO) clippy

.PHONY: all artifact artifacts check qemu qemu-tcg clean

all: artifact

artifact: $(ARTIFACT)

$(ARTIFACT): FORCE
	$(BUILD) --target $(TARGET) --release
	mkdir -p $(dir $(ARTIFACT))
	cp $(BINARY) $(ARTIFACT)

artifacts:
	for arch in $(ARCHS); do $(MAKE) artifact ARCH=$$arch || exit 1; done

check:
	$(CARGO) fmt --check
	$(CLIPPY) --target $(TARGET) --release -- -D warnings
	$(MAKE) artifact

qemu: artifact
	ARCH="$(ARCH)" QEMU="$(QEMU)" QEMU_ACCEL="$(QEMU_ACCEL)" OVMF="$(OVMF)" \
		./scripts/run-qemu.sh $(QEMU_ARGS)

qemu-tcg:
	$(MAKE) qemu QEMU_ACCEL=tcg

clean:
	$(CARGO) clean --target-dir $(TARGET_DIR)
	rm -rf build

.PHONY: FORCE
FORCE:
