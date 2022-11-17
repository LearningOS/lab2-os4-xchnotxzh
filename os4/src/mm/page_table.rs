//! Implementation of [`PageTableEntry`] and [`PageTable`].

use super::{frame_alloc, FrameTracker, PhysPageNum, StepByOne, VirtAddr, VirtPageNum};
use alloc::vec;
use alloc::vec::Vec;
use bitflags::*;

bitflags! {
    /// page table entry flags
    pub struct PTEFlags: u8 {
        const V = 1 << 0;
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
        const G = 1 << 5;
        const A = 1 << 6;
        const D = 1 << 7;
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
/// page table entry structure
pub struct PageTableEntry {
    pub bits: usize,
}

impl PageTableEntry {
    pub fn new(ppn: PhysPageNum, flags: PTEFlags) -> Self {
        PageTableEntry {
            bits: ppn.0 << 10 | flags.bits as usize,
        }
    }
    pub fn empty() -> Self {
        PageTableEntry { bits: 0 }
    }
    pub fn ppn(&self) -> PhysPageNum {
        (self.bits >> 10 & ((1usize << 44) - 1)).into()
    }
    pub fn flags(&self) -> PTEFlags {
        PTEFlags::from_bits(self.bits as u8).unwrap()
    }
    pub fn is_valid(&self) -> bool {
        (self.flags() & PTEFlags::V) != PTEFlags::empty()
    }
    pub fn readable(&self) -> bool {
        (self.flags() & PTEFlags::R) != PTEFlags::empty()
    }
    pub fn writable(&self) -> bool {
        (self.flags() & PTEFlags::W) != PTEFlags::empty()
    }
    pub fn executable(&self) -> bool {
        (self.flags() & PTEFlags::X) != PTEFlags::empty()
    }
}

/// page table structure
pub struct PageTable {
    root_ppn: PhysPageNum,
    frames: Vec<FrameTracker>,
}

/// Assume that it won't oom when creating/mapping.
impl PageTable {
    pub fn new() -> Self {
        let frame = frame_alloc().unwrap();
        PageTable {
            root_ppn: frame.ppn,
            frames: vec![frame],
        }
    }
    /// Temporarily used to get arguments from user space.
    pub fn from_token(satp: usize) -> Self {
        Self {
            root_ppn: PhysPageNum::from(satp & ((1usize << 44) - 1)),
            frames: Vec::new(),
        }
    }
    fn find_pte_create(&mut self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let mut idxs = vpn.indexes();
        let mut ppn = self.root_ppn;
        let mut result: Option<&mut PageTableEntry> = None;
        for (i, idx) in idxs.iter_mut().enumerate() {
            let pte = &mut ppn.get_pte_array()[*idx];
            if i == 2 {
                result = Some(pte);
                break;
            }
            if !pte.is_valid() {
                let frame = frame_alloc().unwrap();
                *pte = PageTableEntry::new(frame.ppn, PTEFlags::V);
                self.frames.push(frame);
            }
            ppn = pte.ppn();
        }
        result
    }
    fn find_pte(&self, vpn: VirtPageNum) -> Option<&PageTableEntry> {
        let idxs = vpn.indexes();
        let mut ppn = self.root_ppn;
        let mut result: Option<&PageTableEntry> = None;
        for (i, idx) in idxs.iter().enumerate() {
            let pte = &ppn.get_pte_array()[*idx];
            if i == 2 {
                result = Some(pte);
                break;
            }
            if !pte.is_valid() {
                return None;
            }
            ppn = pte.ppn();
        }
        result
    }
    #[allow(unused)]
    pub fn map(&mut self, vpn: VirtPageNum, ppn: PhysPageNum, flags: PTEFlags) {
        let pte = self.find_pte_create(vpn).unwrap();
        assert!(!pte.is_valid(), "vpn {:?} is mapped before mapping", vpn);
        *pte = PageTableEntry::new(ppn, flags | PTEFlags::V);
    }
    #[allow(unused)]
    pub fn unmap(&mut self, vpn: VirtPageNum) {
        let pte = self.find_pte_create(vpn).unwrap();
        assert!(pte.is_valid(), "vpn {:?} is invalid before unmapping", vpn);
        *pte = PageTableEntry::empty();
    }
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.find_pte(vpn).copied()
    }
    pub fn token(&self) -> usize {
        8usize << 60 | self.root_ppn.0
    }
}



// /// test and return the res if the type is contained in a single page
// /// 返回能访问到物理数据的引用
// pub fn try_translate_small_type<T>(token: usize, ptr: *const T) -> Option<&'static mut T> {
//     let page_table = PageTable::from_token(token);
//     let va = VirtAddr::from(ptr as usize);
//     let start_offset = va.page_offset();
//     if start_offset + size_of::<T>() > PAGE_SIZE {
//         None
//     } else {
//         let ppn = page_table.translate(va.floor()).unwrap().ppn();
//         let res = PhysAddr::from(ppn).add(start_offset);
//         unsafe { Some((res.0 as *mut T).as_mut().unwrap()) }
//     }
// }

// /// for type so large that spans multiple pages
// /// or even trickier, small type that cross border between 2 pages, unlikely
// /// 返回值 -- 物理字节数组引用的向量
// pub fn translated_large_type<T>(token: usize, ptr: *const T) -> Vec<& 'static mut [u8]> {
//     let ptr = ptr as *const u8;
//     let size = size_of::<T>();
//     translated_byte_buffer(token, ptr, size)
// }



/// translate a pointer to a mutable u8 Vec through page table
pub fn translated_byte_buffer(token: usize, ptr: *const u8, len: usize) -> Vec<&'static mut [u8]> {
    let page_table = PageTable::from_token(token);
    let mut start = ptr as usize;
    let end = start + len;
    let mut v = Vec::new();
    while start < end {
        let start_va = VirtAddr::from(start);
        let mut vpn = start_va.floor();
        let ppn = page_table.translate(vpn).unwrap().ppn();
        vpn.step();
        let mut end_va: VirtAddr = vpn.into();
        end_va = end_va.min(VirtAddr::from(end));
        if end_va.page_offset() == 0 {
            v.push(&mut ppn.get_bytes_array()[start_va.page_offset()..]);
        } else {
            v.push(&mut ppn.get_bytes_array()[start_va.page_offset()..end_va.page_offset()]);
        }
        start = end_va.into();
    }
    v
}

// pub unsafe fn copy_type_into_bufs<T>(value: &T, buffers: Vec<&mut [u8]>) {
//     let value = from_raw_parts(value as *const T as *const u8, size_of::<T>());
//     let mut offset = 0;
//     for buffer in buffers {
//         let dst_len = buffer.len();    
//         buffer.copy_from_slice(&value[offset..offset+dst_len]);
//         offset += dst_len;
//     }
// }

// 复制内核空间数据到用户空间数据
// 参数 -- token: 用户地址空间token，dst_user_va：用户空间目标地址，内核空间源数据地址，len：数据字节长度
pub fn copyout(token: usize, dst_user_va: usize, src: *const u8, len: usize) {
    let page_table = PageTable::from_token(token);
    let start_va = VirtAddr::from(dst_user_va);
    let start_vpn = start_va.floor();
    let start_ppn = page_table.translate(start_vpn).unwrap().ppn();
    let dst = &mut start_ppn.get_bytes_array()[start_va.page_offset()..start_va.page_offset() + len];
    unsafe {
        dst.copy_from_slice(core::slice::from_raw_parts(src, len));
    };
}
