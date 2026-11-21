//! Shop buy/sell, inventory limits, item charges, consumables and the
//! drop-on-death path for the lane-511 item surface.
//!
//! Every item id and every shop id is a shipped row (`ItemData.slk` /
//! `Sellitems=`), and the shop is the real Orc Voodoo Lounge (`ovln`) so the
//! test exercises the same catalogue the mission uses.

use wc3_sim::data::{self, ItemAction, ItemActionError, ItemOutcome};
use wc3_sim::{Sim, UnitId, Vec2};

const SEED: u64 = 51_111;

fn orc_shop(far: bool) -> (Sim, UnitId, UnitId) {
    let mut sim = Sim::new(SEED, 64, 64);
    sim.passive_income = false;
    sim.resources[0] = 5_000;
    sim.lumber[0] = 1_000;
    let shop_pos = if far {
        Vec2::new(2_400.0, 2_400.0)
    } else {
        Vec2::new(700.0, 600.0)
    };
    let shop = sim.spawn_def(0, "ovln", shop_pos).expect("Orc shop");
    // `phea`/`pman`/`stwp` carry `Requires=TWN2` (ItemFunc.txt) with the
    // `ostr` alternative; without a completed tier-2 hall the purchase is
    // refused.  The test buys the real catalogue, so the hall is real too.
    sim.spawn_def(0, "ostr", Vec2::new(400.0, 400.0))
        .expect("Orc stronghold");
    let buyer = sim.spawn_def(0, "Othr", Vec2::new(600.0, 600.0)).expect("Thrall");
    (sim, buyer, shop)
}

fn stock_of(sim: &Sim, shop: UnitId, item: &str) -> i32 {
    sim.shop(shop)
        .and_then(|shop| shop.slots.iter().find(|slot| slot.item == item))
        .map(|slot| slot.stock)
        .unwrap_or(-1)
}

fn give(sim: &mut Sim, unit: UnitId, item: &str, charges: i32) -> wc3_sim::ItemId {
    let pos = sim.unit(unit).unwrap().pos;
    let id = sim
        .spawn_item(item, pos, charges)
        .unwrap_or_else(|| panic!("spawn {item}"));
    sim.item_action(unit, ItemAction::Pickup { item: id })
        .unwrap_or_else(|error| panic!("pickup {item}: {}", error.reason()));
    id
}

/// mechanic `items.shop.buy`: buying a real Orc shop item debits its
/// `goldcost`, consumes one stock unit and lands in the six-slot backpack.
#[test]
fn items_shop_buy_debits_gold_and_decrements_stock() {
    let (mut sim, buyer, shop) = orc_shop(false);
    let gold = sim.resources[0];
    let stock = stock_of(&sim, shop, "phea");
    assert_eq!(stock, 3, "potion of healing stockMax=3");
    let outcome = sim
        .item_action(
            buyer,
            ItemAction::Buy {
                shop,
                item: "phea",
            },
        )
        .expect("buy potion");
    let item = match outcome {
        ItemOutcome::Bought { item, cost, .. } => {
            assert_eq!(cost, 150, "ItemData goldcost");
            item
        }
        other => panic!("unexpected outcome {other:?}"),
    };
    assert_eq!(sim.resources[0], gold - 150);
    assert_eq!(stock_of(&sim, shop, "phea"), stock - 1);
    assert_eq!(sim.unit(buyer).unwrap().items.len(), 1);
    assert!(sim
        .items
        .iter()
        .any(|entry| entry.id == item && entry.owner == Some(0)));
    // Not enough gold is an exact refusal, not a free item.
    sim.resources[0] = 10;
    let error = sim
        .item_action(
            buyer,
            ItemAction::Buy {
                shop,
                item: "phea",
            },
        )
        .expect_err("no gold");
    assert!(matches!(
        error,
        ItemActionError::NotEnoughGold { need: 150, have: 10 }
    ));
}

/// mechanic `items.shop.sell`: a sellable item returns half its `goldcost`,
/// leaves the backpack and goes back on the shop shelf.
#[test]
fn items_shop_sell_refunds_half_and_restocks() {
    let (mut sim, buyer, shop) = orc_shop(false);
    let gold = sim.resources[0];
    let bought = sim
        .item_action(
            buyer,
            ItemAction::Buy {
                shop,
                item: "phea",
            },
        )
        .expect("buy");
    let item = match bought {
        ItemOutcome::Bought { item, .. } => item,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(stock_of(&sim, shop, "phea"), 2, "one sold");
    let outcome = sim
        .item_action(buyer, ItemAction::Sell { item, shop })
        .expect("sell potion");
    assert_eq!(
        outcome,
        ItemOutcome::Sold {
            item,
            shop,
            refund: 75,
        }
    );
    assert_eq!(sim.resources[0], gold - 150 + 75, "half of 150 back");
    assert!(sim.unit(buyer).unwrap().items.is_empty());
    assert!(
        !sim.items.iter().any(|entry| entry.id == item),
        "sold item leaves the world table"
    );
    assert_eq!(stock_of(&sim, shop, "phea"), 3, "sold item restocks the shop");

    // Selling an item the shop does not trade still refunds, but no restock
    // slot is invented for it.
    let claws = give(&mut sim, buyer, "rat3", 0);
    let gold = sim.resources[0];
    let outcome = sim
        .item_action(buyer, ItemAction::Sell { item: claws, shop })
        .expect("sell claws");
    assert_eq!(
        outcome,
        ItemOutcome::Sold {
            item: claws,
            shop,
            refund: 25,
        }
    );
    assert_eq!(sim.resources[0], gold + 25);
}

/// mechanic `items.shop.sell-errors`: non-sellable items, dead/absent shops,
/// out-of-range sales and items that are not carried are exact refusals.
#[test]
fn items_shop_sell_rejects_invalid_sales() {
    let (mut sim, buyer, shop) = orc_shop(false);
    // `tkno` (Tome of Power) is `sellable=false` in ItemData.
    let tome = give(&mut sim, buyer, "tkno", 1);
    assert_eq!(
        sim.item_action(buyer, ItemAction::Sell { item: tome, shop }),
        Err(ItemActionError::NotSellable { item: "tkno" })
    );
    // Not carried.
    let loose = sim.spawn_item("phea", Vec2::new(600.0, 600.0), 1).unwrap();
    assert_eq!(
        sim.item_action(buyer, ItemAction::Sell { item: loose, shop }),
        Err(ItemActionError::NotCarried)
    );
    // Not a shop.
    let grunt = sim.spawn_def(0, "ogru", Vec2::new(600.0, 620.0)).unwrap();
    let potion = give(&mut sim, buyer, "phea", 1);
    assert_eq!(
        sim.item_action(buyer, ItemAction::Sell { item: potion, shop: grunt }),
        Err(ItemActionError::NotAShop)
    );
    // Out of shop range.
    sim.unit_mut(buyer).unwrap().pos = Vec2::new(2_300.0, 2_300.0);
    assert_eq!(
        sim.item_action(buyer, ItemAction::Sell { item: potion, shop }),
        Err(ItemActionError::ShopTooFar)
    );
}

/// mechanic `items.inventory.limit`: six slots are the cap; the seventh item
/// is refused and its passive bonus is not applied.
#[test]
fn items_inventory_six_slots_reject_the_seventh() {
    let (mut sim, buyer, _shop) = orc_shop(false);
    let base_damage = sim.unit(buyer).unwrap().damage;
    let mut ids = Vec::new();
    for index in 0..6 {
        let pos = sim.unit(buyer).unwrap().pos + Vec2::new(index as f32, 0.0);
        let id = sim.spawn_item("rat3", pos, 0).expect("claws");
        sim.item_action(buyer, ItemAction::Pickup { item: id })
            .unwrap_or_else(|error| panic!("pickup {index}: {}", error.reason()));
        ids.push(id);
    }
    assert_eq!(sim.unit(buyer).unwrap().items.len(), 6);
    assert_eq!(sim.unit(buyer).unwrap().damage, base_damage + 18);
    let seventh = sim
        .spawn_item("rat3", sim.unit(buyer).unwrap().pos, 0)
        .expect("claws");
    let error = sim
        .item_action(buyer, ItemAction::Pickup { item: seventh })
        .expect_err("seventh item");
    assert_eq!(error, ItemActionError::InventoryFull);
    assert_eq!(sim.unit(buyer).unwrap().items.len(), 6);
    assert_eq!(
        sim.unit(buyer).unwrap().damage,
        base_damage + 18,
        "refused pickup applied no bonus"
    );
}

/// mechanic `items.death.drop`: a dead carrier drops the whole backpack at the
/// corpse, the passive bonuses go with the items and a second tick cannot
/// duplicate a drop.
#[test]
fn items_death_drops_backpack_and_removes_bonuses() {
    let (mut sim, buyer, _shop) = orc_shop(false);
    let base_damage = sim.unit(buyer).unwrap().damage;
    let claws = give(&mut sim, buyer, "rat3", 0);
    let potion = give(&mut sim, buyer, "phea", 1);
    assert_eq!(sim.unit(buyer).unwrap().damage, base_damage + 3);
    let corpse = sim.unit(buyer).unwrap().pos;
    let killer = sim.spawn_def(1, "ogru", Vec2::new(700.0, 700.0)).unwrap();
    let dealt = sim.apply_damage(killer, buyer, 10_000, data::AttackType::Normal);
    assert!(dealt > 0);
    assert!(!sim.unit(buyer).unwrap().alive);
    // The death path drops the backpack inline; the explicit call and the
    // per-tick safety net must not duplicate anything.
    assert!(sim.unit(buyer).unwrap().items.is_empty());
    assert_eq!(
        sim.unit(buyer).unwrap().damage, base_damage,
        "passive bonus removed on death"
    );
    assert_eq!(sim.drop_items_on_death(buyer), 0, "inline drop is idempotent");
    for id in [claws, potion] {
        let entry = sim.items.iter().find(|entry| entry.id == id).expect("item");
        assert_eq!(entry.owner, None, "item is on the ground");
        assert!(
            entry.pos.distance(corpse) <= 32.0 + 0.5,
            "item lands next to the corpse"
        );
    }
    // The per-tick safety net is idempotent.
    assert_eq!(sim.drop_items_on_death(buyer), 0);
    sim.items_tick();
    assert_eq!(
        sim.items.iter().filter(|entry| entry.owner.is_none()).count(),
        2,
        "no duplicated drops"
    );
}

/// mechanic `items.charges`: every use spends exactly one charge; the last
/// charge removes the item and its passive bonus.
#[test]
fn items_charges_consume_one_per_use_and_remove_at_zero() {
    let (mut sim, buyer, _shop) = orc_shop(false);
    let roast = give(&mut sim, buyer, "shrs", 6);
    let max_hp = sim.unit(buyer).unwrap().max_hp;
    sim.unit_mut(buyer).unwrap().hp = max_hp - 300;
    // Every use spends one charge (`consumed` = a charge was spent); the item
    // itself is removed only when the last charge leaves.
    for expected in (1..=5).rev() {
        let outcome = sim
            .item_action(
                buyer,
                ItemAction::Use {
                    item: roast,
                    target: None,
                    point: None,
                },
            )
            .expect("eat roast");
        assert_eq!(
            outcome,
            ItemOutcome::Used {
                item: roast,
                consumed: true,
                charges_left: expected,
            }
        );
        assert!(sim.unit(buyer).unwrap().items.contains(&roast));
    }
    let last = sim
        .item_action(
            buyer,
            ItemAction::Use {
                item: roast,
                target: None,
                point: None,
            },
        )
        .expect("last charge");
    assert_eq!(
        last,
        ItemOutcome::Used {
            item: roast,
            consumed: true,
            charges_left: 0,
        }
    );
    assert!(sim.unit(buyer).unwrap().items.is_empty());
    assert!(
        !sim.items.iter().any(|entry| entry.id == roast),
        "empty charged item is removed"
    );
    assert_eq!(
        sim.unit(buyer).unwrap().hp,
        max_hp,
        "six 50 hp heals clipped at max"
    );
}

/// mechanic `items.consumable.heal`: a one-charge potion heals its `HP` value,
/// consumes itself and cannot be used twice.
#[test]
fn items_consumable_potion_heals_and_is_consumed() {
    let (mut sim, buyer, _shop) = orc_shop(false);
    let potion = give(&mut sim, buyer, "phea", 1);
    let max_hp = sim.unit(buyer).unwrap().max_hp;
    sim.unit_mut(buyer).unwrap().hp = max_hp - 100;
    let outcome = sim
        .item_action(
            buyer,
            ItemAction::Use {
                item: potion,
                target: None,
                point: None,
            },
        )
        .expect("drink potion");
    assert_eq!(
        outcome,
        ItemOutcome::Used {
            item: potion,
            consumed: true,
            charges_left: 0,
        }
    );
    assert_eq!(sim.unit(buyer).unwrap().hp, max_hp, "heal 250 clipped");
    assert!(sim.unit(buyer).unwrap().items.is_empty());
    assert_eq!(
        sim.item_action(
            buyer,
            ItemAction::Use {
                item: potion,
                target: None,
                point: None,
            }
        ),
        Err(ItemActionError::NotCarried)
    );
    // A permanent stat item is not consumable and has no use path.
    let claws = give(&mut sim, buyer, "rat3", 0);
    assert_eq!(
        sim.item_action(
            buyer,
            ItemAction::Use {
                item: claws,
                target: None,
                point: None,
            }
        ),
        Err(ItemActionError::NoEffect { item: "rat3" })
    );
    assert!(sim.unit(buyer).unwrap().items.contains(&claws));
}
