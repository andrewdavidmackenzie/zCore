# Executor entry function
#
#   void executor_entry()
#
# Simply obtains the executor instance address from the bottom of the stack,
# then jumps to runtime.rs::run_executor.
#
	.text
	.global executor_entry
	.balign 4
executor_entry:
	pop rdi
	push 0 # make rsp 16-bytes aligned
	jmp run_executor
