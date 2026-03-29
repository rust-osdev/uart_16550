.PHONY: default
default: build


.PHONY: build
build:
	cargo build --release \
		--target ./x86-unknown-none.json \
		-Z build-std=core,alloc,compiler_builtins \
		-Z build-std-features=compiler-builtins-mem \
		-Zjson-target-spec
	@# Identical object, different ELF header. Required for Cloud Hypervisor PVH boot.
	objcopy -O elf64-x86-64 target/x86-unknown-none/release/kernel target/x86-unknown-none/release/kernel.elf64

.PHONY: run_qemu
run_qemu: | build
	@# QEMU/TCG allows better debuggability over QEMU/KVM
	qemu-system-i386 \
		-debugcon stdio \
		-device isa-debug-exit,iobase=0xf4,iosize=0x04 \
		-display none \
		-kernel target/x86-unknown-none/release/kernel \
		-m 64M \
		-machine q35,accel=tcg \
		-monitor none \
		-no-reboot \
		-nodefaults \
		-serial file:serial.qemu.pio.txt; \
	rc=$$? ; \
	if [ $$rc -ne 73 ]; then \
		echo "QEMU Test Run Failed. rc=$$rc" >&2 ; \
		exit 1 ; \
	fi
	grep "hello from serial via x86 port I/O" ./serial.qemu.pio.txt



.PHONY: run_chv
run_chv: target/x86-unknown-none/release/kernel.elf64 | build
	@# QEMU/TCG allows better debuggability over QEMU/KVM
	cloud-hypervisor \
	  --cpus boot=1 \
	  --memory size=64M \
	  --console off \
	  --debug-console tty \
	  --serial file=serial.chv.pio.txt \
	  --kernel target/x86-unknown-none/release/kernel.elf64
	rc=$$? ; \
	if [ $$rc -ne 0 ]; then \
		echo "Cloud Hypervisor Test Run Failed. rc=$rc" >&2 ; \
		exit 1 ; \
	fi
	grep "hello from serial via x86 port I/O" ./serial.chv.pio.txt


.PHONY: run
run: | run_chv run_qemu


.PHONY: clean
clean:
	cargo clean
