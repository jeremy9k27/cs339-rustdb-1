use crate::typedef::PageId;
pub(crate) mod table_page;

pub(crate) const INVALID_PAGE_ID: PageId = 0;
pub(crate) const PAGE_SIZE: usize = 4096;
