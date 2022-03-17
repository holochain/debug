use hdk::prelude::*;

entry_defs!(EntryDef {
    id: "test".into(),
    visibility: Default::default(),
    crdt_type: Default::default(),
    required_validations: Default::default(),
    required_validation_type: Default::default(),
});

#[hdk_extern]
fn create(_: ()) -> ExternResult<HeaderHash> {
    let entry_def_id: EntryDefId = "test".into();
    let entry = Entry::app(().try_into().unwrap()).unwrap();
    HDK.with(|h| {
        h.borrow().create(CreateInput::new(
            entry_def_id,
            entry,
            ChainTopOrdering::default(),
        ))
    })
}

#[hdk_extern]
fn read(hash: HeaderHash) -> ExternResult<Option<Element>> {
    get(hash, Default::default())
}
