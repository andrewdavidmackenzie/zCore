# Executor entry function
#
#   void executor_entry()
#
# Simply obtains the executor instance address from the bottom of the stack,
# then jumps to runtime.rs::run_executor.
#
	.text
	.globl executor_entry
	.balign 4
executor_entry:
	ld a0, 0(sp)
	addi sp, sp, 8
	j run_executor


