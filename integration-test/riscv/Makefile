.PHONY: default
default: build

.PHONY: build
build:
	cargo build --offline --release \
		--target riscv64gc-unknown-none-elf \
		-Z build-std=core,alloc,compiler_builtins \
		-Z build-std-features=compiler-builtins-mem

.PHONY: run
run: | build
	qemu-system-riscv64 \
		-bios none \
		-display none \
		-device loader,file=target/riscv64gc-unknown-none-elf/release/kernel-riscv-mmio,cpu-num=0 \
		-m 64M \
		-machine virt \
		-monitor none \
		-no-reboot \
		-nodefaults \
		-serial file:serial.qemu.mmio-riscv.txt
	rc=$$? ; \
	if [ $$rc -ne 0 ]; then \
		echo "QEMU RISC-V MMIO Test Run Failed. rc=$$rc" >&2 ; \
		exit 1 ; \
	fi
	grep "hello from serial via riscv mmio" ./serial.qemu.mmio-riscv.txt

.PHONY: clean
clean:
	cargo clean
