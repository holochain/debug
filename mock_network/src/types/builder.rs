use holochain_types::prelude::{DnaHash, EntryType, HeaderHash};

use super::*;

#[derive(Default)]
pub struct CreateLinkBuilder {
    pub base_address: Option<EntryHash>,
    pub target_address: Option<EntryHash>,
    pub zome_id: Option<ZomeId>,
    pub tag: Option<LinkTag>,
}

#[derive(Default)]
pub struct DeleteLinkBuilder {
    pub link_add_address: Option<HeaderHash>,
    pub base_address: Option<EntryHash>,
}

#[derive(Default)]
pub struct CloseChainBuilder {
    pub new_dna_hash: Option<DnaHash>,
}

#[derive(Default)]
pub struct OpenChainBuilder {
    pub prev_dna_hash: Option<DnaHash>,
}

#[derive(Default)]
pub struct DeleteBuilder {
    pub deletes_address: Option<HeaderHash>,
    pub deletes_entry_address: Option<EntryHash>,
}

#[derive(Default)]
pub struct UpdateBuilder {
    pub original_entry_address: Option<EntryHash>,
    pub original_header_address: Option<HeaderHash>,
    pub entry_type: Option<EntryType>,
    pub entry_hash: Option<EntryHash>,
}
#[derive(Default)]
pub struct CreateBuilder {
    pub entry_type: Option<EntryType>,
    pub entry_hash: Option<EntryHash>,
}

macro_rules! make_from {
    ($t:ident, $v:ident, $( $i:ident ),+) => {
        impl From<$t> for ChainHeader {
            fn from(b: $t) -> Self {
                ChainHeader::$v($v{
                    $( $i: b.$i.unwrap_or_else(|| make(|u| u.arbitrary())) ),+
                })
            }
        }

        impl From<$t> for ChainData {
            fn from(b: $t) -> Self {
                ChainHeader::from(b).into()
            }
        }
    };
}

make_from!(
    CreateLinkBuilder,
    CreateLink,
    base_address,
    target_address,
    zome_id,
    tag
);

make_from!(
    DeleteLinkBuilder,
    DeleteLink,
    link_add_address,
    base_address
);

make_from!(CloseChainBuilder, CloseChain, new_dna_hash);

make_from!(OpenChainBuilder, OpenChain, prev_dna_hash);

make_from!(CreateBuilder, Create, entry_type, entry_hash);

make_from!(
    DeleteBuilder,
    Delete,
    deletes_address,
    deletes_entry_address
);

make_from!(
    UpdateBuilder,
    Update,
    original_entry_address,
    original_header_address,
    entry_type,
    entry_hash
);
