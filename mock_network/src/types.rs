use std::cell::RefCell;

use arbitrary::Unstructured;
use holochain_types::prelude::{
    builder::*, AppEntryBytes, Entry, EntryHash, EntryType, Header, LinkTag, ZomeId,
};
mod builder;
pub use builder::*;

pub struct ChainData {
    pub header: ChainHeader,
    pub entry: Option<Entry>,
}

pub enum ChainHeader {
    InitZomesComplete(InitZomesComplete),
    CreateLink(CreateLink),
    DeleteLink(DeleteLink),
    OpenChain(OpenChain),
    CloseChain(CloseChain),
    Create(Create),
    Update(Update),
    Delete(Delete),
}

impl ChainHeader {
    pub fn build(self, common: HeaderBuilderCommon) -> Header {
        match self {
            ChainHeader::InitZomesComplete(h) => Header::from(h.build(common)),
            ChainHeader::CreateLink(h) => Header::from(h.build(common)),
            ChainHeader::DeleteLink(h) => Header::from(h.build(common)),
            ChainHeader::OpenChain(h) => Header::from(h.build(common)),
            ChainHeader::CloseChain(h) => Header::from(h.build(common)),
            ChainHeader::Create(h) => Header::from(h.build(common)),
            ChainHeader::Update(h) => Header::from(h.build(common)),
            ChainHeader::Delete(h) => Header::from(h.build(common)),
        }
    }
}

impl From<ChainHeader> for ChainData {
    fn from(mut h: ChainHeader) -> Self {
        let entry = match &mut h {
            ChainHeader::InitZomesComplete(_)
            | ChainHeader::CreateLink(_)
            | ChainHeader::DeleteLink(_)
            | ChainHeader::OpenChain(_)
            | ChainHeader::CloseChain(_)
            | ChainHeader::Delete(_) => None,
            ChainHeader::Update(Update {
                entry_type,
                entry_hash,
                ..
            })
            | ChainHeader::Create(Create {
                entry_hash,
                entry_type,
            }) => {
                let entry: AppEntryBytes = make(|u| u.arbitrary());
                let entry = Entry::App(entry);
                let aet = make(|u| u.arbitrary());
                let e_type = EntryType::App(aet);
                let hash = EntryHash::with_data_sync(&entry);
                *entry_hash = hash;
                *entry_type = e_type;
                Some(entry)
            }
        };
        ChainData { header: h, entry }
    }
}

thread_local!(static DATA: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(1000)));

pub fn make<T, F>(f: F) -> T
where
    F: Fn(&mut Unstructured<'_>) -> arbitrary::Result<T>,
{
    let mut r = None;
    let mut len = 0;
    let mut needs_more_data = false;
    for _ in 0..100 {
        DATA.with(|d| {
            let data = d.borrow();
            let mut u = Unstructured::new(data.as_ref());
            let res = f(&mut u);
            match res {
                Ok(t) => {
                    r = Some(t);
                    len = u.len();
                    // break;
                }
                Err(arbitrary::Error::NotEnoughData) => {
                    needs_more_data = true;
                }
                Err(e) => todo!("{:?}", e),
            }
        });
        if needs_more_data {
            DATA.with(|d| {
                // Add more data
                let rng = fastrand::Rng::new();
                d.borrow_mut()
                    .extend(std::iter::repeat_with(|| rng.u8(..)).take(1000));
            });
            needs_more_data = false;
        }
    }
    let t = r.expect("Failed to generate enough data in 100 tries");
    DATA.with(|d| {
        if len == 0 {
            d.borrow_mut().clear();
        } else {
            let mut d = d.borrow_mut();
            match d.len().checked_sub(len) {
                Some(l) => {
                    d.drain(0..l);
                }
                None => d.clear(),
            }
        }
    });
    t
}
