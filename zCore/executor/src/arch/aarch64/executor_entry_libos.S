// Executor entry function (libos mode)
//
// Define both _-prefixed (macOS) and non-prefixed (Linux) symbols.
// Branch target uses _run_executor (macOS) via an indirect call
// to avoid the preprocessor issue with global_asm!.

	.text
	.globl _executor_entry
	.globl executor_entry
	.balign 4
_executor_entry:
executor_entry:
	ldr x0, [sp]
	// Use an indirect branch via the GOT to handle both
	// _run_executor (macOS) and run_executor (Linux).
	// The linker resolves whichever symbol exists.
	bl _run_executor
