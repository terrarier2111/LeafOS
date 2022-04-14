.jump_usermode:
	mov ax, (3 * 8) | 3 ; ring 3 data with bottom 2 bits set for ring 3
	mov ds, ax
	mov es, ax
	mov fs, ax
	mov gs, ax ; SS is handled by iret

	mov rax, rsp
    push (3 * 8) | 3 ; data selector
    push rax ; current esp
    pushf ; eflags
    push (2 * 8) | 3 ; code selector (ring 3 code with bottom 2 bits set for ring 3)
    push rbx ; the desired address to jump to
    iret

;.jump_kernelmode: ; FIXME: Test/Check this!
;	mov ax, (0 * 8) | 0 ; ring 0 data
;	mov ds, ax
;	mov es, ax
;	mov fs, ax
;	mov gs, ax ; SS is handled by iret
;
;	mov rax, rsp
;    push (0 * 8) | 0 ; ring 0 data selector
;    push rax ; current esp
;    pushf ; eflags
;    push (1 * 8) | 0 ; ring 0 code selector
;    push rbx
;    iret