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
	ldr x0, [sp]
	b run_executor


