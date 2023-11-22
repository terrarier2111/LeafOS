[BITS 16]
[ORG 0x7C00]

SetupA20:
# enable A20 line (ensure all memory can be used)
# https://wiki.osdev.org/A20_Line
in al, 0x92
or al, 2
out 0x92, al


# check if cpuid is supported by checking if flipping bit 21
# and pushing that to the flags register will keep bit 21 set
# when checking bit 21 again
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

# Now check for extended CpuId Support
CheckExtCpuId:
mov eax, 0x80000000
cpuid
cmp eax, 0x80000001
jb NoCpuId
mov eax, 0x80000001
cpuid
test edx, LONG_MODE_SUPPORTED_FLAG
jz NoLongMode

# address a specific msr
mov ecx, 0xC0000080
rdmsr
or eax, 



# disable interrupts
cli
lgdt [GDT] # load global descriptor table
mov eax, cr0
or al, PROTECTION_ENABLED_FLAG
mov cr0, eax
jmp 08h:ProtectionMain

ProtectionMain:

# clear 4 adjacent HW pages to use as the page table (just below 0x1000)
ClearPages:
mov edi, PAGE_TABLE_START
mov ecx, PAGE_SIZE
xor eax, eax
ret stosd
# store destination into cr3
mov edi, cr3

SetupTables:
# PML4T - 0x1000.
# PDPT  - 0x2000.
# PDT   - 0x3000.
# PT    - 0x4000. 

mov DWORD [edi], 0x2003
add edi, 0x1000
mov DWORD [edi], 0x3003
add edi, 0x1000
mov DWORD [edi], 0x4003
add edi, 0x1000
mov ebx, PAGE_PRESENT_FLAG | PAGE_WRITABLE_FLAG
# there are 512 entries per table
mov ecx, 512
SetEntry:
mov dw [edi], ebx
add ebx, 0x1000
add edi, 8
loop SetEntry

EnablePAEPaging:
mov eax, cr4
or eax, PAE_ENABLED_FLAG
mov cr4, eax





# TODO: setup paging!

# updates the static value of level 5 paging availability
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
# TODO: print message saying there's no CPUId support and the OS doesn't work without
jmp Idle

NoLongMode:
# TODO: print message saying there's no long mode support and the OS doesn't work without
jmp Idle


LONG_MODE_ENABLED_FLAG equ 1 << 8
PAE_ENABLED_FLAG equ 1 << 5
PAGE_PRESENT_FLAG equ 1 << 0
PAGE_WRITABLE_FLAG equ 1 << 1
PAGE_SIZE equ 4096
PAGE_TABLE_START equ 0x10000
ZERO_FLAG equ 1 << 6
LONG_MODE_SUPPORTED_FLAG equ 1 << 29
LEVEL_5_PAGING_FLAG equ 1 << 16
PROTECTION_ENABLED_FLAG equ 0x1
CPUID_TEST equ 0x200000

GDT:
GDT_NULL DQ 0 # Null-segment
GDT_CODE DW 0xFFFFh,
         DW 0,
         DB 0,
         DB 10011010b,
         DB 1100111b,
         DB 0
GDT_DATA DW 0xFFFFh,
         DW 0,
         DB 0,
         DB 10010010b,
         DB 11001111b,
         DB 0
GDT_END

GDT_DESC     DB GDT_END - GDT,
             DW GDT
LEVEL5_PAGING_AVAILABLE DW 0

