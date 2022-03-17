use holochain::core::Timestamp;
use holochain_state::prelude::*;
use holochain_types::{
    dht_op::{produce_op_lights_from_elements, DhtOpType, OpOrder, UniqueForm},
    prelude::{Element, EntryVisibility, ValidationStatus},
};
use rusqlite::{params, CachedStatement, ToSql, Transaction};

// pub fn bulk_insert_element_as_authority<'a>(
//     txn: &mut Transaction,
//     elements: impl Iterator<Item = &'a Element>,
// ) {
//     let stmt = txn
//         .prepare_cached(
//             "INSERT INTO DhtOp (hash, type, basis_hash, header_hash,
//                     storage_center_loc, authored_timestamp, op_order,
//                     validation_status, when_integrated, require_receipt,
//                     num_validation_attempts, last_validation_attempt, dependency)
//                     VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
//         )
//         .unwrap();
//     for element in elements {
//         if let Some(entry) = element.entry().as_option() {
//             insert_entry(txn, element.header().entry_hash().as_ref().unwrap(), entry).unwrap();
//         }
//         insert_header(txn, element.signed_header()).unwrap();
//     }
// }

pub fn insert_element_as_authority(txn: &mut Transaction, element: &Element) {
    if let Some(entry) = element.entry().as_option() {
        insert_entry(txn, element.header().entry_hash().as_ref().unwrap(), entry).unwrap();
    }
    insert_header(txn, element.signed_header()).unwrap();
    let mut stmt = txn
        .prepare_cached(
            "INSERT INTO DhtOp (hash, type, basis_hash, header_hash, storage_center_loc, authored_timestamp, op_order, validation_status, when_integrated, require_receipt, num_validation_attempts, last_validation_attempt, dependency) 
                    VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .unwrap();
    commit_ops(&mut stmt, element);
}

fn commit_ops(stmt: &mut CachedStatement<'_>, element: &Element) {
    for ops in produce_op_lights_from_elements(vec![element]) {
        for op in ops {
            let op_type = op.get_type();
            if matches!(op_type, DhtOpType::StoreEntry)
                && element.header().entry_type().map_or(false, |et| {
                    matches!(et.visibility(), EntryVisibility::Private)
                })
            {
                continue;
            }
            let op_hash = UniqueForm::op_hash(op_type, element.header().clone())
                .unwrap()
                .1;
            let basis_hash = op.dht_basis();
            let header_hash = element.header_address();
            let storage_center_loc = basis_hash.get_loc();
            let authored_timestamp = element.header().timestamp();
            let op_order = OpOrder::new(op_type, authored_timestamp);
            let validation_status = ValidationStatus::Valid;
            let when_integrated = Timestamp::now();
            let require_receipt = false;
            let num_validation_attempts: i32 = 1;
            let last_validation_attempt = Timestamp::now();
            let dep = get_dependency(op_type, element.header());
            let dependency = match &dep {
                Dependency::Header(h) => Some(h.to_sql().unwrap()),
                Dependency::Entry(e) => Some(e.to_sql().unwrap()),
                Dependency::Null => None,
            };

            stmt.execute(params![
                op_hash,
                op_type,
                basis_hash,
                header_hash,
                storage_center_loc,
                authored_timestamp,
                op_order,
                validation_status,
                when_integrated,
                require_receipt,
                num_validation_attempts,
                last_validation_attempt,
                dependency
            ])
            .unwrap();
        }
    }
}
