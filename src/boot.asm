[BITS 16]
[ORG 0x7C00]

ALIGN 4
IDT:
    dw 0
    dd 0

Main:
SetupA20:
    ; enable A20 line (ensure all memory can be used)
    ; https://wiki.osdev.org/A20_Line
    in al, 0x92
    or al, 2
    out 0x92, al


; check if cpuid is supported by checking if flipping bit 21
; and pushing that to the flags register will keep bit 21 set
; when checking bit 21 again
CheckCpuId:
    pushfd
    pop eax
    mov eax, ecx
    xor eax, 1 << 21
    push eax
    popfd
    pushfd
    pop eax
    push ecx
    popfd
    xor eax, ecx
    jz NoCpuId

; Now check for extended CpuId Support
CheckExtCpuId:
    mov eax, 0x80000000
    cpuid
    cmp eax, 0x80000001
    jb NoCpuId
    mov eax, 0x80000001
    cpuid
    test edx, LONG_MODE_SUPPORTED_FLAG
    jz NoLongMode

; clear 4 adjacent HW pages to use as the page table (just below 0x1000)
ClearPages:
mov edi, PAGE_TABLE_START
mov ecx, PAGE_SIZE
xor eax, eax
rep stosd
; store destination into cr3
mov edi, cr3

SetupTables:
; PML4T - 0x1000.
; PDPT  - 0x2000.
; PDT   - 0x3000.
; PT    - 0x4000. 

mov DWORD [edi], 0x2003
add edi, 0x1000
mov DWORD [edi], 0x3003
add edi, 0x1000
mov DWORD [edi], 0x4003
add edi, 0x1000
mov ebx, PAGE_PRESENT_FLAG | PAGE_WRITABLE_FLAG
; there are 512 entries per table
mov ecx, 512
SetEntry:
mov [edi], ebx
add ebx, 0x1000
add edi, 8
loop SetEntry

; disable irqs
mov al, 0xFF
out 0xA1, al
out 0x21, al

nop
nop

lidt [IDT]

; Enable PAE and Paging
mov eax, PAE_ENABLED_FLAG | PGE_ENABLED_FLAG
mov cr4, eax

; set cr3 to PML4
mov edx, 0x1000
mov cr3, edx

; read from EFER MSR
mov ecx, 0xC0000080
rdmsr

or eax, LONG_MODE_ENABLED_FLAG
wrmsr

; FIXME: the following 3 lines of code contain a bug (or at least trigger it)
mov ebx, cr0
or ebx, PROTECTION_ENABLED_FLAG | PAGING_ENABLED_FLAG
mov cr0, ebx

Idle2:
jmp Idle2

lgdt [GDT_DESC]

jmp CODE_SEG:LongMode

; TODO: setup paging!

; updates the static value of level 5 paging availability
CheckLevel5Paging:
mov eax, 0x7
xor ecx, ecx
cpuid
test ecx, LEVEL_5_PAGING_FLAG
pushfd
pop ebx
and ebx, LEVEL_5_PAGING_FLAG
shr ebx, 6
mov edx, 1
sub edx, ebx
mov [LEVEL5_PAGING_AVAILABLE], edx



Idle:
jmp Idle

NoCpuId:
; TODO: print message saying there's no CPUId support and the OS doesn't work without
jmp Idle

NoLongMode:
; TODO: print message saying there's no long mode support and the OS doesn't work without
jmp Idle


LONG_MODE_ENABLED_FLAG equ 1 << 8
PAE_ENABLED_FLAG equ 1 << 5
PGE_ENABLED_FLAG equ 1 << 7
PAGE_PRESENT_FLAG equ 1 << 0
PAGE_WRITABLE_FLAG equ 1 << 1
PAGE_SIZE equ 4096
PAGE_TABLE_START equ 0x10000
ZERO_FLAG equ 1 << 6
LONG_MODE_SUPPORTED_FLAG equ 1 << 29
LEVEL_5_PAGING_FLAG equ 1 << 16
PAGING_ENABLED_FLAG equ 0x80000000
PROTECTION_ENABLED_FLAG equ 0x1
CPUID_TEST equ 0x200000
CODE_SEG equ 0x0008
DATA_SEG equ 0x0010

GDT:
GDT_NULL: DQ 0 ; Null-segment
GDT_CODE: DW 0xFFFF,
         DW 0,
         DB 0,
         DB 10011010b,
         DB 1100111b,
         DB 0
GDT_DATA: DW 0xFFFF,
         DW 0,
         DB 0,
         DB 10010010b,
         DB 11001111b,
         DB 0
GDT_END

ALIGN 4
    DW 0

GDT_DESC:
    DB GDT_END - GDT,
    DW GDT
LEVEL5_PAGING_AVAILABLE DW 0

; a piece of test code
[BITS 64]      
LongMode:
    mov ax, DATA_SEG
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
 
    ; Blank out the screen to a blue color.
    mov edi, 0xB8000
    mov rcx, 500                      ; Since we are clearing uint64_t over here, we put the count as Count/4.
    mov rax, 0x1F201F201F201F20       ; Set the value to set the screen to: Blue background, white foreground, blank spaces.
    rep stosq                         ; Clear the entire screen. 
 
    ; Display "Hello World!"
    mov edi, 0x00b8000              
 
    mov rax, 0x1F6C1F6C1F651F48    
    mov [edi],rax
 
    mov rax, 0x1F6F1F571F201F6F
    mov [edi + 8], rax
 
    mov rax, 0x1F211F641F6C1F72
    mov [edi + 16], rax
 
    Loop:
    jmp Loop

; Pad out file.
times 510 - ($-$$) db 0
dw 0xAA55