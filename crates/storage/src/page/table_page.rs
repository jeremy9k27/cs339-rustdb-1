use crate::frame_handle::{PageFrameMutHandle, PageFrameRefHandle};
use crate::page::PAGE_SIZE;
use crate::record_id::RecordId;
use crate::Result;
use crate::{frame::PageFrame, typedef::PageId};
use bytemuck::{Pod, Zeroable};
use rustdb_catalog::tuple::Tuple;
use rustdb_error::Error;
use std::mem;
use std::ops::{Deref, DerefMut};

#[repr(C)]
#[derive(Pod, Zeroable, Copy, Clone)]
pub(crate) struct TablePageHeader {
    next_page_id: PageId,
    tuple_cnt: u32,
    deleted_tuple_cnt: u32,
    _padding: [u8; 4],
}

#[repr(C)]
#[derive(Pod, Zeroable, Copy, Clone)]
pub(crate) struct TupleInfo {
    offset: u16,
    size_bytes: u16,
    metadata: TupleMetadata,
}

impl TupleInfo {
    pub(crate) fn offset(&self) -> u16 {
        self.offset
    }

    pub(crate) fn size_bytes(&self) -> u16 {
        self.size_bytes
    }
}

pub(crate) const TABLE_PAGE_HEADER_SIZE: usize = mem::size_of::<TablePageHeader>();
pub(crate) const TUPLE_INFO_SIZE: usize = mem::size_of::<TupleInfo>();

#[repr(C)]
#[derive(Pod, Zeroable, Copy, Clone)]
pub struct TupleMetadata {
    is_deleted: u8,
    _padding: [u8; 1],
}

impl TupleMetadata {
    pub(crate) fn new(is_deleted: bool) -> Self {
        Self {
            is_deleted: is_deleted as u8,
            _padding: [0; 1],
        }
    }

    pub(crate) fn is_deleted(&self) -> bool {
        self.is_deleted != 0
    }

    pub(crate) fn set_deleted(&mut self, deleted: bool) {
        self.is_deleted = deleted as u8;
    }
}

/// Generic struct for both mutable and immutable table pages.
pub struct TablePage<T> {
    page_frame_handle: T,
}

impl<T: Deref<Target = PageFrame>> TablePage<T> {
    pub(crate) fn page_id(&self) -> PageId {
        self.page_frame_handle.page_id()
    }

    pub(crate) fn next_page_id(&self) -> PageId {
        self.header().next_page_id
    }

    pub(crate) fn tuple_count(&self) -> u32 {
        self.header().tuple_cnt
    }

    /// Immutable access to the header
    pub(crate) fn header(&self) -> &TablePageHeader {
        bytemuck::from_bytes(&self.page_frame_handle.data()[..TABLE_PAGE_HEADER_SIZE])
    }

    /// Returns the slot array (immutable)
    pub(crate) fn slot_array(&self) -> &[TupleInfo] {
        let tuple_cnt = self.header().tuple_cnt as usize;
        let slots_end = TABLE_PAGE_HEADER_SIZE + (tuple_cnt * TUPLE_INFO_SIZE);
        bytemuck::cast_slice(&self.page_frame_handle.data()[TABLE_PAGE_HEADER_SIZE..slots_end])
    }

    pub(crate) fn get_tuple(&self, rid: &RecordId) -> Result<(TupleMetadata, Tuple)> {
        let slot_id = rid.slot_id();
        
        let tuple_info = self.slot_array()[slot_id as usize];
        let tuple_offset = tuple_info.offset;
        let tuple_size = tuple_info.size_bytes;
        let tuple_metadata = tuple_info.metadata;

        use bytes::Bytes;

        let tuple = Tuple::new(Bytes::copy_from_slice(&self.page_frame_handle.data()[tuple_offset as usize..(tuple_offset + tuple_size) as usize]));
        Ok((tuple_metadata, tuple))
    }

    fn get_next_tuple_offset(&mut self, tuple: &Tuple) -> Result<u16> {
        let num_tuples = self.tuple_count();
        
        if num_tuples == 0 {
            return Ok((PAGE_SIZE - &tuple.tuple_size()) as u16)
        }
        
        let last_tuple_offset = self.slot_array()[(num_tuples - 1) as usize].offset;
        
        Ok(last_tuple_offset - tuple.tuple_size() as u16)
    }

    fn validate_record_id(&self, rid: &RecordId) -> Result<()> {
        if rid.page_id() != self.page_id() || rid.slot_id() >= self.tuple_count() {
            Err(Error::InvalidInput(rid.to_string()))
        } else {
            Ok(())
        }
    }
}

impl<T: DerefMut<Target = PageFrame> + Deref<Target = PageFrame>> TablePage<T> {
    /// Mutable access to the header
    pub(crate) fn header_mut(&mut self) -> &mut TablePageHeader {
        bytemuck::from_bytes_mut(&mut self.page_frame_handle.data_mut()[..TABLE_PAGE_HEADER_SIZE])
    }

    /// Returns the slot array (mutable)
    pub(crate) fn slot_array_mut(&mut self) -> &mut [TupleInfo] {
        let tuple_cnt = self.header().tuple_cnt as usize;
        let slots_end = TABLE_PAGE_HEADER_SIZE + (tuple_cnt * TUPLE_INFO_SIZE);
        bytemuck::cast_slice_mut(
            &mut self.page_frame_handle.data_mut()[TABLE_PAGE_HEADER_SIZE..slots_end],
        )
    }

    pub(crate) fn init_header(&mut self, next_page_id: PageId) {
        let header = self.header_mut();
        *header = TablePageHeader {
            next_page_id,
            tuple_cnt: 0,
            deleted_tuple_cnt: 0,
            _padding: [0; 4],
        };
    }

    pub(crate) fn set_next_page_id(&mut self, next_page_id: PageId) {
        let header = self.header_mut();
        header.next_page_id = next_page_id;
    }

    pub(crate) fn set_tuple_count(&mut self, tuple_count: u32) {
        let header = self.header_mut();
        header.tuple_cnt = tuple_count;
    }

    pub(crate) fn insert_tuple(&mut self, meta: &TupleMetadata, tuple: &Tuple) -> Result<RecordId> {
        let offset = self.get_next_tuple_offset(&tuple)?;

        //write to num_tuples in mutable header
        self.header_mut().tuple_cnt += 1;
        let tuple_cnt = self.header().tuple_cnt;
        
        // update slot array
        self.slot_array_mut()[(tuple_cnt-1) as usize].offset = offset;
        self.slot_array_mut()[(tuple_cnt-1) as usize].size_bytes = tuple.tuple_size() as u16;
        self.slot_array_mut()[(tuple_cnt-1) as usize].metadata = *meta;
        
        
        // write to page bytes?
        self.page_frame_handle.data_mut()[offset as usize..offset as usize + tuple.tuple_size()].copy_from_slice(&tuple.data());
        
        // ret the record id
        let record_id = RecordId::new(self.page_id(), tuple_cnt-1);
        Ok(record_id)
    }

    pub(crate) fn update_tuple_metadata(
        &mut self,
        rid: &RecordId,
        metadata: TupleMetadata,
    ) -> Result<()> {

        let slot_id = rid.slot_id();
        let mut tuple_info = self.slot_array()[slot_id as usize];
        
        tuple_info.metadata.set_deleted(metadata.is_deleted());
        
        Ok(())
    }
}

/// Type alias for immutable TablePage
pub type TablePageRef<'a> = TablePage<PageFrameRefHandle<'a>>;
/// Type alias for mutable TablePage
pub type TablePageMut<'a> = TablePage<PageFrameMutHandle<'a>>;

impl<'a> From<PageFrameRefHandle<'a>> for TablePageRef<'a> {
    fn from(page_frame_handle: PageFrameRefHandle<'a>) -> Self {
        TablePage { page_frame_handle }
    }
}

impl<'a> From<PageFrameMutHandle<'a>> for TablePageMut<'a> {
    fn from(page_frame_handle: PageFrameMutHandle<'a>) -> Self {
        TablePage { page_frame_handle }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, RwLock};

    use rustdb_catalog::tuple::Tuple;
    use serial_test::serial;

    use crate::{
        buffer_pool::BufferPoolManager, disk::disk_manager::DiskManager,
        replacer::lru_k_replacer::LrukReplacer,
    };

    use super::*;

    fn get_bpm_with_pool_size(pool_size: usize) -> BufferPoolManager {
        let disk_manager = Arc::new(Mutex::new(DiskManager::new("test.db").unwrap()));
        let replacer = Box::new(LrukReplacer::new(5));
        BufferPoolManager::new(pool_size, disk_manager, replacer)
    }

    fn get_bpm_arc_with_pool_size(pool_size: usize) -> Arc<RwLock<BufferPoolManager>> {
        Arc::new(RwLock::new(get_bpm_with_pool_size(pool_size)))
    }

    #[test]
    pub fn test_insert_tuple() {
        let bpm = get_bpm_arc_with_pool_size(10);
        let frame_handle = BufferPoolManager::create_page_handle(&bpm).unwrap();
        let mut table_page = TablePageMut::from(frame_handle);

        table_page.init_header(1);

        let tuple = Tuple::new(vec![1_u8, 2_u8, 3_u8, 4_u8].into());
        let meta = TupleMetadata::new(false);
        let slot = table_page.insert_tuple(&meta, &tuple).unwrap();

        assert_eq!(1, table_page.tuple_count());
        assert_eq!(1, table_page.next_page_id());

        let rid = RecordId::new(table_page.page_id(), slot.slot_id());
        assert_eq!(tuple.data(), table_page.get_tuple(&rid).unwrap().1.data());
    }

    #[test]
    #[serial]
    fn test_table_page_metadata() {
        let bpm = get_bpm_arc_with_pool_size(10);

        let page_id;
        {
            let frame_handle = BufferPoolManager::create_page_handle(&bpm).unwrap();
            let mut table_page = TablePageMut::from(frame_handle);

            table_page.init_header(2);

            page_id = table_page.page_id();

            let header = table_page.header();
            assert_eq!(header.next_page_id, 2);
            assert_eq!(header.tuple_cnt, 0);
            assert_eq!(header.deleted_tuple_cnt, 0);

            table_page.header_mut().tuple_cnt = 5;

            let updated_header = table_page.header();
            assert_eq!(updated_header.tuple_cnt, 5);

            let slots = table_page.slot_array();
            assert_eq!(slots.len(), 5);

            let slots_mut = table_page.slot_array_mut();
            slots_mut[0].offset = 55;
            slots_mut[1].offset = 11;
            slots_mut[1].metadata.set_deleted(true);
            assert_eq!(slots_mut[0].offset, 55);
            assert_eq!(slots_mut[1].offset, 11);
            assert_eq!(slots_mut[1].metadata.is_deleted(), true);

            table_page.header_mut().tuple_cnt = 3;

            let slots = table_page.slot_array();
            assert_eq!(slots.len(), 3);
            assert_eq!(slots[0].offset, 55);
            assert_eq!(slots[1].offset, 11);
            assert_eq!(slots[1].metadata.is_deleted(), true);
        }

        let frame_handle_1 = BufferPoolManager::fetch_page_handle(&bpm, page_id).unwrap();

        let table_page1 = TablePageRef::from(frame_handle_1);

        assert_eq!(1, table_page1.page_id());
        assert_eq!(2, table_page1.next_page_id());
        assert_eq!(3, table_page1.tuple_count());

        let slots = table_page1.slot_array();
        assert_eq!(slots.len(), 3);
        assert_eq!(slots[0].offset, 55);
        assert_eq!(slots[1].offset, 11);
        assert_eq!(slots[1].metadata.is_deleted(), true);
    }

    #[test]
    fn test_insert_and_get_tuple() {
        let bpm = get_bpm_arc_with_pool_size(10);

        let page_id;
        let insert_record_id;

        // tuple metadata
        let metadata = TupleMetadata::new(true);

        let tuple_data = vec![1, 2, 3, 1, 2, 3, 4, 5, 6, 7, 8];
        {
            let frame_handle = BufferPoolManager::create_page_handle(&bpm).unwrap();
            let mut table_page = TablePageMut::from(frame_handle);

            page_id = table_page.page_id();

            // Initialize page header
            table_page.init_header(2);
            assert_eq!(table_page.header().tuple_cnt, 0);

            let tuple = Tuple::new(tuple_data.clone().into());

            // Insert the tuple
            let record_id = table_page.insert_tuple(&metadata, &tuple).unwrap();
            assert_eq!(table_page.tuple_count(), 1);

            insert_record_id = record_id.clone();

            // Retrieve the tuple
            let (retrieved_meta, retrieved_tuple) = table_page.get_tuple(&record_id).unwrap();

            // Ensure retrieved tuple matches inserted tuple
            assert_eq!(retrieved_meta.is_deleted(), metadata.is_deleted());
            assert_eq!(retrieved_tuple.data(), &tuple_data);
        }
        let frame_handle_1 = BufferPoolManager::fetch_page_handle(&bpm, page_id).unwrap();

        let table_page1 = TablePageRef::from(frame_handle_1);
        // Retrieve the tuple
        let (retrieved_meta, retrieved_tuple) = table_page1.get_tuple(&insert_record_id).unwrap();

        // Ensure retrieved tuple matches inserted tuple
        assert_eq!(retrieved_meta.is_deleted(), metadata.is_deleted());
        assert_eq!(retrieved_tuple.data(), &tuple_data);
    }

}
