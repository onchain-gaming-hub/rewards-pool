//! Layout contract: sizes and fixed offsets other programs / indexers rely on.
use {
    anchor_lang::{prelude::Pubkey, AccountSerialize, Space},
    rewards_pool::state::*,
};

#[test]
fn account_sizes_match_spec() {
    assert_eq!(8 + MatchPool::INIT_SPACE, 1_580);
    assert_eq!(8 + PoolAsset::INIT_SPACE, 203);
    assert_eq!(8 + BoostReceipt::INIT_SPACE, 162);
}

#[test]
fn rent_payer_at_8_and_match_pool_at_40() {
    let (rp, mp) = (Pubkey::new_unique(), Pubkey::new_unique());
    let a = PoolAsset {
        rent_payer: rp, match_pool: mp, version: 1, kind: AssetKind::Sol, asset_id: Pubkey::default(),
        token_program: Pubkey::default(), decimals: 9, base_amount: 0, boost_received: 0, claimed: 0,
        base_returned: false, boost_refunded: 0, fee_collected: false, bump: 0, vault_bump: 0, _reserved: [0; 32],
    };
    let mut buf = Vec::new();
    a.try_serialize(&mut buf).unwrap();
    assert_eq!(&buf[8..40], rp.as_ref());
    assert_eq!(&buf[40..72], mp.as_ref());
    let r = BoostReceipt { rent_payer: rp, match_pool: mp, version: 1, booster: rp, asset_id: mp, amount: 1, bump: 0, _reserved: [0; 16] };
    let mut buf = Vec::new();
    r.try_serialize(&mut buf).unwrap();
    assert_eq!(&buf[8..40], rp.as_ref());
    assert_eq!(&buf[40..72], mp.as_ref());
}
