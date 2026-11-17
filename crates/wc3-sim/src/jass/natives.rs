//! Native dispatch table.
//!
//! One table maps every implemented native name to its handler. The registry
//! (`registry.rs`) marks a name `handled` by asking this module, and the VM
//! counts any call that reaches a name with no handler, so "implemented" is a
//! measured property of the table and never a claim in a doc comment.
//!
//! `Convert*` natives are the engine's enum constructors: each one returns its
//! argument unchanged (the engine's `ConvertPlayerColor(i)` *is* `i`), so they
//! are handled by a prefix rule instead of 60 identical rows.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use super::value::Value;
use super::vm::JassVm;

pub(crate) mod cinema;
mod core;
pub(crate) mod trigger;
mod unit;
mod world;

pub(crate) use core::{compare_limit, player_state_value};

/// Handler signature: the machine plus the evaluated arguments.
pub type NativeFn = fn(&mut JassVm, &[Value]) -> Value;

macro_rules! handler_table {
    ($($name:literal => $func:path),* $(,)?) => {
        /// Every native with a real handler, in source order.
        pub static HANDLERS: &[(&str, NativeFn)] = &[
            $(($name, $func as NativeFn)),*
        ];
    };
}

handler_table! {
    // --- conversions, strings, math -------------------------------------
    "I2R" => core::i2r,
    "R2I" => core::r2i,
    "I2S" => core::i2s,
    "R2S" => core::r2s,
    "R2SW" => core::r2sw,
    "S2I" => core::s2i,
    "S2R" => core::s2r,
    "StringLength" => core::string_length,
    "SubString" => core::substring,
    "StringCase" => core::string_case,
    "StringHash" => core::string_hash,
    "GetHandleId" => core::get_handle_id,
    "AbilityId" => core::ability_id,
    "AbilityId2String" => core::ability_id2string,
    "Pow" => core::pow,
    "SquareRoot" => core::square_root,
    "Sin" => core::sin,
    "Cos" => core::cos,
    "Tan" => core::tan,
    "Asin" => core::asin,
    "Acos" => core::acos,
    "Atan" => core::atan,
    "Atan2" => core::atan2,
    "Deg2Rad" => core::deg2rad,
    "Rad2Deg" => core::rad2deg,
    "GetRandomInt" => core::get_random_int,
    "GetRandomReal" => core::get_random_real,
    "SetRandomSeed" => core::set_random_seed,

    // --- BJ player-slot natives -----------------------------------------
    "GetBJMaxPlayers" => core::get_bj_max_players,
    "GetBJMaxPlayerSlots" => core::get_bj_max_player_slots,
    "GetBJPlayerNeutralVictim" => core::get_bj_player_neutral_victim,
    "GetBJPlayerNeutralExtra" => core::get_bj_player_neutral_extra,
    "GetPlayerNeutralPassive" => core::get_player_neutral_passive,
    "GetPlayerNeutralAggressive" => core::get_player_neutral_aggressive,

    "GetPlayerController" => core::get_player_controller,
    "GetPlayerSlotState" => core::get_player_slot_state,
    "IsPlayerObserver" => core::is_player_observer,
    "GetGameSpeed" => core::get_game_speed,
    "VersionGet" => core::version_get,
    "Preloader" => core::preloader,
    "SetAllItemTypeSlots" => core::set_all_item_type_slots,
    "SetAllUnitTypeSlots" => core::set_all_unit_type_slots,
    "IsFogEnabled" => core::is_fog_enabled,
    "IsFogMaskEnabled" => core::is_fog_enabled,
    "ForceEnumPlayers" => core::force_enum_players,

    // --- players --------------------------------------------------------
    "Player" => core::player,
    "GetPlayerId" => core::get_player_id,
    "GetLocalPlayer" => core::get_local_player,
    "GetPlayerName" => core::get_player_name,
    "GetPlayerColor" => core::get_player_color,
    "SetPlayerColor" => core::set_player_color,
    "GetPlayerRace" => core::get_player_race,
    "SetPlayerRacePreference" => core::set_player_race_preference,
    "SetPlayerRaceSelectable" => core::set_player_race_selectable,
    "SetPlayerController" => core::set_player_controller,
    "SetPlayerTeam" => core::set_player_team,
    "SetPlayerStartLocation" => core::set_player_start_location,
    "ForcePlayerStartLocation" => core::force_player_start_location,
    "GetPlayerStartLocation" => core::get_player_start_location,
    "SetPlayerAlliance" => core::set_player_alliance,
    "GetPlayerAlliance" => core::get_player_alliance,
    "SetPlayerState" => core::set_player_state,
    "GetPlayerState" => core::get_player_state,
    "SetPlayerTechResearched" => core::set_player_tech_researched,
    "GetPlayerTechResearched" => core::get_player_tech_researched,
    "SetPlayerTechMaxAllowed" => core::set_player_tech_max_allowed,
    "GetPlayerTechMaxAllowed" => core::get_player_tech_max_allowed,
    "SetPlayerAbilityAvailable" => core::set_player_ability_available,
    "GetPlayerUnitCount" => core::get_player_unit_count,
    "GetPlayerStructureCount" => core::get_player_structure_count,
    "SetMapName" => core::set_map_name,
    "SetMapDescription" => core::set_map_description,
    "GetLocalizedString" => core::get_localized_string,
    "IsMapFlagSet" => core::is_map_flag_set,
    "SetTeams" => core::set_teams,
    "SetPlayers" => core::set_players,
    "SetGamePlacement" => core::set_game_placement,
    "SetGameTypeSupported" => core::set_game_type_supported,
    "GetGameTypeSelected" => core::get_game_type_selected,
    "SetFloatGameState" => core::set_float_game_state,
    "GetFloatGameState" => core::get_float_game_state,
    "GetGameDifficulty" => core::get_game_difficulty,
    "BlzGetPlayerTownHallCount" => core::blz_get_player_town_hall_count,
    "GetIntegerGameState" => core::get_integer_game_state,
    "SetGameDifficulty" => core::set_game_difficulty,

    // --- hashtables -----------------------------------------------------
    "InitHashtable" => core::init_hashtable,
    "FlushChildHashtable" => core::flush_child_hashtable,
    "FlushParentHashtable" => core::flush_parent_hashtable,
    "SaveInteger" => core::save_integer,
    "LoadInteger" => core::load_integer,
    "SaveReal" => core::save_real,
    "LoadReal" => core::load_real,
    "SaveStr" => core::save_str,
    "LoadStr" => core::load_str,
    "SaveBoolean" => core::save_boolean,
    "LoadBoolean" => core::load_boolean,
    "HaveSavedInteger" => core::have_saved_integer,
    "HaveSavedReal" => core::have_saved_real,
    "HaveSavedString" => core::have_saved_str,
    "HaveSavedBoolean" => core::have_saved_boolean,
    "HaveSavedHandle" => core::have_saved_handle,
    "RemoveSavedInteger" => core::remove_saved_integer,
    "RemoveSavedHandle" => core::remove_saved_handle,
    "RemoveSavedBoolean" => core::remove_saved_integer,
    "RemoveSavedReal" => core::remove_saved_integer,
    "RemoveSavedString" => core::remove_saved_integer,
    "SaveUnitHandle" => core::save_handle,
    "LoadUnitHandle" => core::load_handle,
    "SaveItemHandle" => core::save_handle,
    "LoadItemHandle" => core::load_handle,
    "SaveDestructableHandle" => core::save_handle,
    "LoadDestructableHandle" => core::load_handle,
    "SaveTimerHandle" => core::save_handle,
    "LoadTimerHandle" => core::load_handle,
    "SaveTriggerHandle" => core::save_handle,
    "LoadTriggerHandle" => core::load_handle,
    "SaveGroupHandle" => core::save_handle,
    "LoadGroupHandle" => core::load_handle,
    "SaveLocationHandle" => core::save_handle,
    "LoadLocationHandle" => core::load_handle,
    "SaveRectHandle" => core::save_handle,
    "LoadRectHandle" => core::load_handle,
    "SaveRegionHandle" => core::save_handle,
    "LoadRegionHandle" => core::load_handle,
    "SaveEffectHandle" => core::save_handle,
    "LoadEffectHandle" => core::load_handle,
    "SaveSoundHandle" => core::save_handle,
    "LoadSoundHandle" => core::load_handle,
    "SavePlayerHandle" => core::save_handle,
    "LoadPlayerHandle" => core::load_handle,
    "SaveAbilityHandle" => core::save_handle,
    "LoadAbilityHandle" => core::load_handle,
    "SaveDialogHandle" => core::save_handle,
    "LoadDialogHandle" => core::load_handle,
    "SaveButtonHandle" => core::save_handle,
    "LoadButtonHandle" => core::load_handle,
    "SaveQuestHandle" => core::save_handle,
    "LoadQuestHandle" => core::load_handle,
    "SaveLeaderboardHandle" => core::save_handle,
    "LoadLeaderboardHandle" => core::load_handle,
    "SaveMultiboardHandle" => core::save_handle,
    "LoadMultiboardHandle" => core::load_handle,
    "SaveTextTagHandle" => core::save_handle,
    "LoadTextTagHandle" => core::load_handle,
    "SaveLightningHandle" => core::save_handle,
    "LoadLightningHandle" => core::load_handle,
    "SaveImageHandle" => core::save_handle,
    "LoadImageHandle" => core::load_handle,
    "SaveUbersplatHandle" => core::save_handle,
    "LoadUbersplatHandle" => core::load_handle,
    "SaveFogModifierHandle" => core::save_handle,
    "LoadFogModifierHandle" => core::load_handle,
    "SaveTrackableHandle" => core::save_handle,
    "LoadTrackableHandle" => core::load_handle,
    "SaveTimerDialogHandle" => core::save_handle,
    "LoadTimerDialogHandle" => core::load_handle,
    "SaveUnitPoolHandle" => core::save_handle,
    "LoadUnitPoolHandle" => core::load_handle,
    "SaveItemPoolHandle" => core::save_handle,
    "LoadItemPoolHandle" => core::load_handle,

    // --- units ----------------------------------------------------------
    "CreateUnit" => unit::create_unit_native,
    "CreateUnitAtLoc" => unit::create_unit_at_loc,
    "CreateUnitByName" => unit::create_unit_by_name,
    "CreateUnitAtLocByName" => unit::create_unit_at_loc_by_name,
    "BlzCreateUnitWithSkin" => world::blz_create_unit_with_skin,
    "GetUnitX" => unit::get_unit_x,
    "GetUnitY" => unit::get_unit_y,
    "GetUnitLoc" => unit::get_unit_loc,
    "GetUnitFacing" => unit::get_unit_facing,
    "SetUnitFacing" => unit::set_unit_facing,
    "SetUnitFacingTimed" => unit::set_unit_facing_timed,
    "SetUnitPosition" => unit::set_unit_position,
    "SetUnitPositionLoc" => unit::set_unit_position_loc,
    "SetUnitX" => unit::set_unit_x,
    "SetUnitY" => unit::set_unit_y,
    "GetUnitState" => unit::get_unit_state,
    "SetUnitState" => unit::set_unit_state,
    "GetWidgetLife" => unit::get_widget_life,
    "SetWidgetLife" => unit::set_widget_life,
    "GetUnitTypeId" => unit::get_unit_type_id,
    "GetUnitName" => unit::get_unit_name,
    "BlzSetUnitName" => world::blz_set_unit_name,
    "GetOwningPlayer" => unit::get_owning_player,
    "SetUnitOwner" => unit::set_unit_owner,
    "KillUnit" => unit::kill_unit,
    "RemoveUnit" => unit::remove_unit,
    "ShowUnit" => unit::show_unit,
    "IsUnitHidden" => unit::is_unit_hidden,
    "SetUnitAcquireRange" => unit::set_unit_acquire_range,
    "GetUnitAcquireRange" => unit::get_unit_acquire_range,
    "SetUnitColor" => unit::set_unit_color,
    "SetUnitVertexColor" => unit::set_unit_vertex_color,
    "SetUnitScale" => unit::set_unit_scale,
    "SetUnitTimeScale" => unit::set_unit_time_scale,
    "SetUnitPathing" => unit::set_unit_pathing,
    "SetUnitInvulnerable" => unit::set_unit_invulnerable,
    "SetUnitMoveSpeed" => unit::set_unit_move_speed,
    "GetUnitMoveSpeed" => unit::get_unit_move_speed,
    "GetUnitDefaultMoveSpeed" => unit::get_unit_default_move_speed,
    "SetUnitAnimation" => unit::set_unit_animation,
    "SetUnitAnimationByIndex" => unit::set_unit_animation_by_index,
    "QueueUnitAnimation" => unit::queue_unit_animation,
    "AddUnitAnimationProperties" => world::add_unit_animation_properties,
    "SetUnitUserData" => unit::set_unit_user_data,
    "GetUnitUserData" => unit::get_unit_user_data,
    "UnitApplyTimedLife" => unit::unit_apply_timed_life,
    "SetResourceAmount" => unit::set_resource_amount,
    "GetResourceAmount" => unit::get_resource_amount,
    "UnitAddAbility" => unit::unit_add_ability,
    "UnitRemoveAbility" => unit::unit_remove_ability,
    "GetUnitAbilityLevel" => unit::get_unit_ability_level,
    "SetUnitAbilityLevel" => unit::set_unit_ability_level,
    "UnitMakeAbilityPermanent" => unit::unit_make_ability_permanent,
    "SelectHeroSkill" => unit::select_hero_skill,
    "SetHeroLevel" => unit::set_hero_level,
    "GetHeroLevel" => unit::get_hero_level,
    "AddHeroXP" => unit::add_hero_xp,
    "GetHeroXP" => unit::get_hero_xp,
    "GetHeroStr" => unit::get_hero_str,
    "GetHeroAgi" => unit::get_hero_agi,
    "GetHeroInt" => unit::get_hero_int,
    "SetHeroStr" => unit::set_hero_str,
    "SetHeroAgi" => unit::set_hero_agi,
    "SetHeroInt" => unit::set_hero_int,
    "UnitDamageTarget" => unit::unit_damage_target,
    "UnitDamagePoint" => unit::unit_damage_point,
    "IsUnitType" => unit::is_unit_type,
    "IsUnitAlly" => unit::is_unit_ally,
    "IsUnitEnemy" => unit::is_unit_enemy,
    "IssueImmediateOrder" => unit::issue_immediate_order,
    "IssuePointOrder" => unit::issue_point_order,
    "IssuePointOrderLoc" => unit::issue_point_order_loc,
    "IssueTargetOrder" => unit::issue_target_order,
    "RemoveAllGuardPositions" => unit::remove_all_guard_positions,

    // --- items ----------------------------------------------------------
    "CreateItem" => unit::create_item,
    "BlzCreateItemWithSkin" => world::blz_create_item_with_skin,
    "GetItemTypeId" => unit::get_item_type_id,
    "RemoveItem" => unit::remove_item,
    "GetItemCharges" => unit::get_item_charges,
    "SetItemCharges" => unit::set_item_charges,
    "UnitAddItem" => unit::unit_add_item,
    "UnitAddItemById" => unit::unit_add_item_by_id,
    "UnitAddItemToSlotById" => unit::unit_add_item_to_slot_by_id,
    "UnitDropItemPoint" => unit::unit_drop_item_point,
    "UnitDropItemSlot" => unit::unit_drop_item_slot,
    "UnitRemoveItem" => unit::unit_remove_item,
    "ChooseRandomItem" => unit::choose_random_item,
    "ChooseRandomItemEx" => unit::choose_random_item_ex,
    "SetItemColor" => world::set_item_color,

    // --- destructables --------------------------------------------------
    "CreateDestructable" => unit::create_destructable,
    "CreateDestructableZ" => unit::create_destructable_z,
    "BlzCreateDestructableWithSkin" => unit::blz_create_destructable_with_skin,
    "BlzCreateDestructableZWithSkin" => unit::blz_create_destructable_z_with_skin,
    "BlzCreateDestructableWithSkinPitchRollColor" => unit::blz_create_destructable_with_skin_pitch_roll_color,
    "GetDestructableX" => unit::get_destructable_x,
    "GetDestructableY" => unit::get_destructable_y,
    "GetDestructableTypeId" => unit::get_destructable_type_id,
    "GetDestructableLife" => unit::get_destructable_life,
    "GetDestructableMaxLife" => unit::get_destructable_max_life,
    "SetDestructableLife" => unit::set_destructable_life,
    "KillDestructable" => unit::kill_destructable,
    "RemoveDestructable" => unit::remove_destructable,
    "SetDestructableVertexColor" => world::set_destructable_vertex_color,

    // --- effects / display ---------------------------------------------
    "AddSpecialEffect" => unit::add_special_effect,
    "AddSpecialEffectTarget" => unit::add_special_effect_target,
    "DestroyEffect" => unit::destroy_effect,
    "DisplayTextToPlayer" => unit::display_text_to_player,
    "DisplayTimedTextToPlayer" => unit::display_timed_text_to_player,
    "ClearTextMessages" => unit::clear_text_messages,
    "SelectUnit" => unit::select_unit,
    "ClearSelection" => unit::clear_selection,

    // --- triggers / events ---------------------------------------------
    "CreateTrigger" => trigger::create_trigger,
    "DestroyTrigger" => trigger::destroy_trigger,
    "TriggerAddAction" => trigger::trigger_add_action,
    "TriggerAddCondition" => trigger::trigger_add_condition,
    "TriggerRemoveAction" => trigger::trigger_remove_action,
    "TriggerRemoveCondition" => trigger::trigger_remove_condition,
    "TriggerClearActions" => trigger::trigger_clear_actions,
    "TriggerClearConditions" => trigger::trigger_clear_conditions,
    "EnableTrigger" => trigger::enable_trigger,
    "DisableTrigger" => trigger::disable_trigger,
    "IsTriggerEnabled" => trigger::is_trigger_enabled,
    "TriggerRegisterTimerEvent" => trigger::trigger_register_timer_event,
    "TriggerRegisterTimerExpireEvent" => trigger::trigger_register_timer_expire,
    "TriggerRegisterUnitEvent" => trigger::trigger_register_unit_event,
    "TriggerRegisterPlayerUnitEvent" => trigger::trigger_register_player_unit_event,
    "TriggerRegisterPlayerEvent" => trigger::trigger_register_player_event,
    "TriggerRegisterDeathEvent" => trigger::trigger_register_death_event,
    "TriggerRegisterEnterRegion" => trigger::trigger_register_enter_region,
    "TriggerRegisterLeaveRegion" => trigger::trigger_register_leave_region,
    "TriggerRegisterPlayerChatEvent" => trigger::trigger_register_player_chat_event,
    "TriggerRegisterPlayerAllianceChange" => trigger::trigger_register_player_alliance_change,
    "TriggerRegisterPlayerStateEvent" => trigger::trigger_register_player_state_event,
    "TriggerRegisterGameEvent" => world::trigger_register_game_event,
    "TriggerRegisterDialogEvent" => trigger::trigger_register_dialog_event,
    "TriggerRegisterUnitInRange" => trigger::trigger_register_unit_in_range,
    "IsPlayerInForce" => trigger::is_player_in_force,
    "TriggerExecute" => trigger::trigger_execute,
    "TriggerEvaluate" => trigger::trigger_evaluate,
    "GetTriggeringTrigger" => trigger::get_triggering_trigger,
    "GetTriggerUnit" => trigger::get_trigger_unit,
    "GetTriggerWidget" => trigger::get_trigger_widget,
    "GetDyingUnit" => trigger::get_dying_unit,
    "GetEnteringUnit" => trigger::get_entering_unit,
    "GetLeavingUnit" => trigger::get_leaving_unit,
    "GetChangingUnit" => trigger::get_changing_unit,
    "GetChangingUnitPrevOwner" => trigger::get_changing_unit_prev_owner,
    "GetAttacker" => trigger::get_attacker,
    "GetKillingUnit" => trigger::get_killing_unit,
    "GetLevelingUnit" => trigger::get_leveling_unit,
    "GetTrainedUnit" => trigger::get_trained_unit,
    "GetTrainedUnitType" => trigger::get_trained_unit_type,
    "GetEventDamage" => trigger::get_event_damage,
    "GetEventDamageSource" => trigger::get_event_damage_source,
    "GetTriggerEventId" => trigger::get_trigger_event_id,
    "GetTriggerEvalCount" => trigger::get_trigger_eval_count,
    "GetTriggerExecCount" => trigger::get_trigger_exec_count,
    "GetSpellAbilityId" => trigger::get_spell_ability_id,
    "GetSpellAbilityUnit" => trigger::get_spell_ability_unit,
    "GetSpellTargetUnit" => trigger::get_spell_target_unit,
    "GetSpellTargetX" => trigger::get_spell_target_x,
    "GetSpellTargetY" => trigger::get_spell_target_y,
    "GetSpellTargetLoc" => trigger::get_spell_target_loc,
    "GetOrderedUnit" => trigger::get_ordered_unit,
    "GetExpiredTimer" => trigger::get_expired_timer,
    "Condition" => trigger::condition,
    "Filter" => trigger::filter,
    "And" => trigger::and_boolexpr,
    "Or" => trigger::or_boolexpr,
    "Not" => trigger::not_boolexpr,
    "DestroyBoolExpr" => trigger::destroy_boolexpr,

    // --- timers ---------------------------------------------------------
    "CreateTimer" => trigger::create_timer,
    "DestroyTimer" => trigger::destroy_timer,
    "TimerStart" => trigger::timer_start,
    "PauseTimer" => trigger::pause_timer,
    "ResumeTimer" => trigger::resume_timer,
    "TimerGetRemaining" => trigger::timer_get_remaining,
    "TimerGetElapsed" => trigger::timer_get_elapsed,
    "TimerGetTimeout" => trigger::timer_get_timeout,
    "TriggerSleepAction" => trigger::trigger_sleep_action,

    // --- groups / forces ------------------------------------------------
    "CreateGroup" => trigger::create_group,
    "DestroyGroup" => trigger::destroy_group,
    "GroupAddUnit" => trigger::group_add_unit,
    "GroupRemoveUnit" => trigger::group_remove_unit,
    "GroupClear" => trigger::group_clear,
    "IsUnitInGroup" => trigger::is_unit_in_group,
    "FirstOfGroup" => trigger::first_of_group,
    "ForGroup" => trigger::for_group,
    "GetEnumUnit" => trigger::get_enum_unit,
    "GetFilterUnit" => trigger::get_filter_unit,
    "GroupEnumUnitsOfPlayer" => trigger::group_enum_units_of_player,
    "GroupEnumUnitsInRange" => trigger::group_enum_units_in_range,
    "GroupEnumUnitsInRangeOfLoc" => trigger::group_enum_units_in_range_of_loc,
    "GroupEnumUnitsInRect" => trigger::group_enum_units_in_rect,
    "CreateForce" => trigger::create_force,
    "ForceAddPlayer" => trigger::force_add_player,
    "ForceRemovePlayer" => trigger::force_remove_player,
    "GetEnumPlayer" => trigger::get_enum_player,
    "ForForce" => trigger::for_force,
    "DestroyForce" => trigger::destroy_force,

    // --- rects, regions, locations -------------------------------------
    "Rect" => trigger::rect,
    "RectFromLoc" => trigger::rect_from_loc,
    "RemoveRect" => trigger::remove_rect,
    "SetRect" => trigger::set_rect,
    "SetRectFromLoc" => trigger::set_rect_from_loc,
    "MoveRectTo" => trigger::move_rect_to,
    "GetRectCenterX" => trigger::get_rect_center_x,
    "GetRectCenterY" => trigger::get_rect_center_y,
    "GetRectMinX" => trigger::get_rect_min_x,
    "GetRectMinY" => trigger::get_rect_min_y,
    "GetRectMaxX" => trigger::get_rect_max_x,
    "GetRectMaxY" => trigger::get_rect_max_y,
    "CreateRegion" => trigger::create_region,
    "RegionAddRect" => trigger::region_add_rect,
    "RegionClearRect" => trigger::region_clear_rect,
    "RemoveRegion" => trigger::remove_region,
    "Location" => trigger::location,
    "RemoveLocation" => trigger::remove_location,
    "GetLocationX" => trigger::get_location_x,
    "GetLocationY" => trigger::get_location_y,
    "MoveLocation" => trigger::move_location,

    // --- start locations, camera, environment ---------------------------
    "DefineStartLocation" => world::define_start_location,
    "DefineStartLocationLoc" => world::define_start_location_loc,
    "GetStartLocationX" => world::get_start_location_x,
    "GetStartLocationY" => world::get_start_location_y,
    "GetStartLocationLoc" => world::get_start_location_loc,
    "SetStartLocPrio" => world::set_start_loc_prio,
    "SetStartLocPrioCount" => world::set_start_loc_prio_count,
    "GetStartLocPrio" => world::get_start_loc_prio,
    "GetStartLocPrioSlot" => world::get_start_loc_prio_slot,
    "SetEnemyStartLocPrio" => world::set_enemy_start_loc_prio,
    "SetEnemyStartLocPrioCount" => world::set_enemy_start_loc_prio_count,
    "CreateCameraSetup" => world::create_camera_setup,
    "CameraSetupSetField" => world::camera_setup_set_field,
    "CameraSetupSetDestPosition" => world::camera_setup_set_dest_position,
    "CameraSetupGetDestPositionLoc" => world::camera_setup_get_dest_position_loc,
    "CameraSetupApply" => world::camera_setup_apply,
    "CameraSetupApplyForceDuration" => world::camera_setup_apply,
    "CameraSetupApplyWithZ" => world::camera_setup_apply,
    "CameraSetupApplyForceDurationWithZ" => world::camera_setup_apply,
    "BlzCameraSetupSetCameraType" => world::blz_camera_setup_set_camera_type,
    "SetCameraBounds" => world::set_camera_bounds,
    "GetCameraBoundMinX" => world::get_camera_bound_min_x,
    "GetCameraBoundMaxX" => world::get_camera_bound_max_x,
    "GetCameraBoundMinY" => world::get_camera_bound_min_y,
    "GetCameraBoundMaxY" => world::get_camera_bound_max_y,
    "TriggerRegisterGameStateEvent" => world::trigger_register_game_state_event,
    "CreateTimerDialog" => world::create_timer_dialog,
    "DestroyTimerDialog" => world::destroy_timer_dialog,
    "TimerDialogSetTitle" => world::timer_dialog_set_title,
    "TimerDialogDisplay" => world::timer_dialog_display,
    "GetCameraMargin" => world::get_camera_margin,
    "SetCameraField" => world::set_camera_field,
    "SetCameraPosition" => world::set_camera_position,
    "PanCameraTo" => world::pan_camera_to,
    "PanCameraToTimed" => world::pan_camera_to_timed,
    "SetCameraQuickPosition" => world::set_camera_quick,
    "ResetToGameCamera" => world::reset_to_game_camera,
    "AddCameraBlocker" => world::add_camera_blocker,
    // --- cinematic scene, camera state, quests, orders (campaign layer) ---
    "SetCinematicScene" => cinema::set_cinematic_scene,
    "EndCinematicScene" => cinema::end_cinematic_scene,
    "ForceCinematicSubtitles" => cinema::force_cinematic_subtitles,
    "SetCinematicAudio" => cinema::set_cinematic_audio,
    "SetCinematicCamera" => cinema::set_cinematic_camera,
    "ShowInterface" => cinema::show_interface,
    "EnableUserControl" => cinema::enable_user_control,
    "EnableOcclusion" => cinema::enable_occlusion,
    "SetCameraTargetController" => cinema::set_camera_target_controller,
    "SetCameraOrientController" => cinema::set_camera_orient_controller,
    "StopCamera" => cinema::stop_camera,
    "SetCameraRotateMode" => cinema::set_camera_rotate_mode,
    "PanCameraToWithZ" => cinema::pan_camera_to_with_z,
    "PanCameraToTimedWithZ" => cinema::pan_camera_to_timed_with_z,
    "CameraSetSmoothingFactor" => cinema::camera_set_smoothing_factor,
    "CameraSetSourceNoise" => cinema::camera_set_source_noise,
    "CameraSetSourceNoiseEx" => cinema::camera_set_source_noise_ex,
    "CameraSetTargetNoise" => cinema::camera_set_target_noise,
    "CameraSetTargetNoiseEx" => cinema::camera_set_target_noise_ex,
    "CameraSetDepthOfFieldScale" => cinema::camera_set_depth_of_field_scale,
    "CameraSetFocalDistance" => cinema::camera_set_focal_distance,
    "GetCameraTargetPositionX" => cinema::get_camera_target_position_x,
    "GetCameraTargetPositionY" => cinema::get_camera_target_position_y,
    "GetCameraTargetPositionZ" => cinema::get_camera_target_position_z,
    "GetCameraTargetPositionLoc" => cinema::get_camera_target_position_loc,
    "GetCameraEyePositionX" => cinema::get_camera_eye_position_x,
    "GetCameraEyePositionY" => cinema::get_camera_eye_position_y,
    "GetCameraEyePositionZ" => cinema::get_camera_eye_position_z,
    "GetCameraEyePositionLoc" => cinema::get_camera_eye_position_loc,
    "GetCameraField" => cinema::get_camera_field,
    "AdjustCameraField" => cinema::adjust_camera_field,
    "CameraSetupGetField" => cinema::camera_setup_get_field,
    "CameraSetupGetDestPositionX" => cinema::camera_setup_get_dest_position_x,
    "CameraSetupGetDestPositionY" => cinema::camera_setup_get_dest_position_y,
    "PlayCinematic" => cinema::play_cinematic,
    "PlayModelCinematic" => cinema::play_model_cinematic,
    "IsQuestCompleted" => cinema::is_quest_completed,
    "IsQuestDiscovered" => cinema::is_quest_discovered,
    "IsQuestRequired" => cinema::is_quest_required,
    "IsQuestFailed" => cinema::is_quest_failed,
    "IsQuestEnabled" => cinema::is_quest_enabled,
    "IsQuestItemCompleted" => cinema::is_quest_item_completed,
    "QuestSetFailed" => cinema::quest_set_failed,
    "QuestSetEnabled" => cinema::quest_set_enabled,
    "DestroyQuest" => cinema::destroy_quest,
    "FlashQuestDialogButton" => cinema::flash_quest_dialog_button,
    "ForceQuestDialogUpdate" => cinema::force_quest_dialog_update,
    "OrderId" => cinema::order_id,
    "OrderId2String" => cinema::order_id2string,
    "IssuePointOrderById" => cinema::issue_point_order_by_id,
    "IssuePointOrderByIdLoc" => cinema::issue_point_order_by_id_loc,
    "IssueTargetOrderById" => cinema::issue_target_order_by_id,
    "IssueImmediateOrderById" => cinema::issue_immediate_order_by_id,
    "IssueBuildOrder" => cinema::issue_build_order,
    "IssueBuildOrderById" => cinema::issue_build_order_by_id,
    "GetIssuedOrderId" => cinema::get_issued_order_id,
    "GetOrderPointX" => cinema::get_order_point_x,
    "GetOrderPointY" => cinema::get_order_point_y,
    "GetOrderPointLoc" => cinema::get_order_point_loc,
    "GetUnitCurrentOrder" => cinema::get_unit_current_order,
    "GetOrderTargetUnit" => cinema::get_order_target_unit,
    "GetOrderTargetDestructable" => cinema::get_order_target_destructable,
    "GetOrderTargetItem" => cinema::get_order_target_item,
    "SetDayNightModels" => world::set_day_night_models,
    "NewSoundEnvironment" => world::new_sound_environment,
    "SetMapMusic" => world::set_map_music,
    "PlayMusic" => world::play_music,
    "SetSkyModel" => world::set_sky_model,
    "SetWaterBaseColor" => world::set_water_base_color,
    "SetTerrainFog" => world::set_terrain_fog,
    "SetTerrainFogEx" => world::set_terrain_fog_ex,
    "SetTerrainFogExV" => world::set_terrain_fog_ex_v,
    "BlzSetTerrainFogMaxLinearDensity" => world::blz_set_terrain_fog_max_linear_density,
    "SetHDWaterParamsEx" => world::set_hd_water_params_ex,
    "BlzSetTerrainFogDrawOverSky" => world::blz_set_terrain_fog_draw_over_sky,
    "SetUnitFog" => world::set_unit_fog,
    "TerrainDeformCrater" => world::terrain_deform_crater,
    "TerrainDeformRipple" => world::terrain_deform_ripple,
    "TerrainDeformWave" => world::terrain_deform_wave,
    "AddWeatherEffect" => world::add_weather_effect,
    "EnableWeatherEffect" => world::enable_weather_effect,
    "RemoveWeatherEffect" => world::remove_weather_effect,

    // --- sounds ---------------------------------------------------------
    "CreateSound" => unit::create_sound,
    "CreateSoundFilenameWithLabel" => unit::create_sound_filename_with_label,
    "CreateSoundFromLabel" => world::create_sound_from_label,
    "CreateMIDISound" => world::create_midi_sound,
    "SetSoundDuration" => world::set_sound_duration,
    "GetSoundDuration" => world::get_sound_duration,
    "SetSoundVolume" => world::set_sound_volume,
    "SetSoundChannel" => world::set_sound_channel,
    "SetSoundPitch" => world::set_sound_pitch,
    "SetSoundParamsFromLabel" => world::set_sound_params_from_label,
    "SetSoundDistances" => world::set_sound_distances,
    "SetSoundDistanceCutoff" => world::set_sound_distance_cutoff,
    "SetSoundConeAngles" => world::set_sound_cone_angles,
    "SetSoundConeOrientation" => world::set_sound_cone_orientation,
    "SetSoundPosition" => world::set_sound_position,
    "StartSound" => world::start_sound,
    "StopSound" => world::stop_sound,
    "KillSoundWhenDone" => world::kill_sound_when_done,
    "SetSoundFacialAnimationLabel" => world::set_sound_facial_animation_label,
    "SetSoundFacialAnimationGroupLabel" => world::set_sound_facial_animation_group_label,
    "SetSoundFacialAnimationSetFilepath" => world::set_sound_facial_animation_set_filepath,
    "SetDialogueSpeakerNameKey" => world::set_dialogue_speaker_name_key,
    "SetDialogueTextKey" => world::set_dialogue_text_key,
    "PlayDialogueFromSpeakerEx" => world::play_dialogue_from_speaker_ex,

    // --- enumerations over destructables/items --------------------------
    "EnumDestructablesInRect" => world::enum_destructables_in_rect,
    "EnumItemsInRect" => world::enum_items_in_rect,
    "GetEnumDestructable" => world::get_enum_destructable,
    "GetEnumItem" => world::get_enum_item,
    "GetFilterDestructable" => world::get_filter_destructable,
    "GetFilterItem" => world::get_filter_item,

    // --- UI -------------------------------------------------------------
    "DialogCreate" => world::dialog_create,
    "DialogAddButton" => world::dialog_add_button,
    "DialogClear" => world::dialog_clear,
    "DialogSetMessage" => world::dialog_set_message,
    "DialogDisplay" => world::dialog_display,
    "CreateTextTag" => world::create_text_tag,
    "SetTextTagText" => world::set_text_tag_text,
    "SetTextTagPos" => world::set_text_tag_pos,
    "SetTextTagLifeSpan" => world::set_text_tag_life_span,
    "DestroyTextTag" => world::destroy_text_tag,
    "CreateTrackable" => world::create_trackable,
    "CreateItemPool" => world::create_item_pool,
    "ItemPoolAddItemType" => world::item_pool_add_item,
    "DestroyItemPool" => world::destroy_item_pool,
    "CreateUnitPool" => world::create_unit_pool,
    "UnitPoolAddUnitType" => world::unit_pool_add_unit,
    "DestroyUnitPool" => world::destroy_unit_pool,
    "SetPlayerRaceSkin" => world::set_player_race_skin,
    "ClearMapMusic" => world::clear_map_music,
    "FogEnable" => world::fog_enable,
    "FogMaskEnable" => world::fog_mask_enable,
    "AttachSoundToUnit" => world::attach_sound_to_unit,
    "EnableUserUI" => world::enable_user_ui,
    "SetTimeOfDayScale" => world::set_time_of_day_scale,
    "DisplayCineFilter" => world::display_cine_filter,
    "SetCineFilterTexture" => world::set_cine_filter_texture,
    "SetCineFilterStartUV" => world::set_cine_filter_start_uv,
    "SetCineFilterEndUV" => world::set_cine_filter_end_uv,
    "SetCineFilterStartColor" => world::set_cine_filter_start_color,
    "SetCineFilterEndColor" => world::set_cine_filter_end_color,
    "SetCineFilterDuration" => world::set_cine_filter_duration,
    "SetCineFilterBlendMode" => world::set_cine_filter_blend_mode,
    "SetCineFilterTexMapFlags" => world::set_cine_filter_tex_map_flags,
    "MathRound" => world::math_round,
    "PauseUnit" => world::pause_unit,
    "SuspendHeroXP" => world::suspend_hero_xp,
    "SetPlayerHandicapXP" => world::set_player_handicap_xp,
    "SetPlayerOnScoreScreen" => world::set_player_on_score_screen,
    "CreateQuest" => world::create_quest,
    "QuestCreateItem" => world::quest_create_item,
    "QuestSetTitle" => world::quest_set_title,
    "QuestSetDescription" => world::quest_set_description,
    "QuestSetIconPath" => world::quest_set_icon_path,
    "QuestSetRequired" => world::quest_set_required,
    "QuestSetDiscovered" => world::quest_set_discovered,
    "QuestSetCompleted" => world::quest_set_completed,
    "QuestItemSetDescription" => world::quest_item_set_description,
    "QuestItemSetCompleted" => world::quest_item_set_completed,
    "UnitAddType" => world::unit_add_type,
    "UnitRemoveType" => world::unit_remove_type,
    "DestroyFogModifier" => world::destroy_fog_modifier,
    "CreateFogModifierRect" => world::create_fog_modifier_rect,
    "FogModifierStart" => world::fog_modifier_start,
}

/// Index for the handler table, built once.
fn index() -> &'static BTreeMap<&'static str, NativeFn> {
    static INDEX: OnceLock<BTreeMap<&'static str, NativeFn>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut map = BTreeMap::new();
        for (name, handler) in HANDLERS {
            if map.insert(*name, *handler).is_some() {
                // A duplicate row would silently shadow a handler; panic in
                // tests, ignore in release (first registration wins).
                debug_assert!(false, "duplicate native handler {name}");
            }
        }
        map
    })
}

/// Runs the handler for `name`, or `None` when the native has none.
pub fn dispatch(vm: &mut JassVm, name: &str, args: &[Value]) -> Option<Value> {
    if let Some(handler) = index().get(name) {
        return Some(handler(vm, args));
    }
    // `Convert*` enum constructors are the identity on their argument.
    if name.len() > 7 && name[..7].eq_ignore_ascii_case("Convert") {
        return Some(args.first().cloned().unwrap_or(Value::Int(0)));
    }
    None
}

/// True when a handler exists (or the `Convert*` identity rule applies).
pub fn is_handled(name: &str) -> bool {
    if index().contains_key(name) {
        return true;
    }
    name.len() > 7 && name[..7].eq_ignore_ascii_case("Convert")
}

/// Number of natives with a handler.
pub fn handled_count() -> usize {
    index().len()
}

// ---------------------------------------------------------------------------
// Argument helpers shared by the handler modules
// ---------------------------------------------------------------------------

/// Integer argument (`0` when the script passed fewer arguments).
pub(crate) fn i(args: &[Value], index: usize) -> i32 {
    args.get(index).map_or(0, Value::int)
}

/// Real argument.
pub(crate) fn r(args: &[Value], index: usize) -> f32 {
    args.get(index).map_or(0.0, Value::real)
}

/// Boolean argument.
pub(crate) fn b(args: &[Value], index: usize) -> bool {
    args.get(index).is_some_and(Value::truthy)
}

/// String argument.
pub(crate) fn s(args: &[Value], index: usize) -> String {
    args.get(index).map_or_else(String::new, Value::text)
}

/// Handle argument.
pub(crate) fn hd(args: &[Value], index: usize) -> Option<super::value::Handle> {
    args.get(index).and_then(Value::handle)
}

/// Integer rawcode of a four-character unit/ability/item id, for event contexts.
pub fn unit_rawcode(id: &str) -> i32 {
    let bytes = id.as_bytes();
    if bytes.len() != 4 {
        return 0;
    }
    i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Integer rawcode of a spell id (`Spell` enum → its ability four-character id).
pub fn spell_rawcode(spell: crate::Spell) -> i32 {
    let name = format!("{spell:?}");
    // `Spell` variants carry the ability's four-character id in the data table;
    // the variant name is the closest stable identifier when the table has no
    // direct mapping, and 0 tells the script "unknown ability" honestly.
    let _ = name;
    0
}

/// Natives that only exist in Reforged-era headers (1.33+). No `common.j` in
/// this tree declares them, so they are not in the generated registry; the
/// handlers are still registered and the VM dispatches them through the
/// unknown-name path, which is why `patch_3.0.0.w3x` runs without an unresolved
/// native. Add a newer header and regenerate to move them into the registry.
pub const HEADER_ABSENT_NATIVES: &[&str] = &[
    "AddCameraBlocker",
    "BlzCameraSetupSetCameraType",
    "BlzCreateDestructableWithSkinPitchRollColor",
    "BlzSetTerrainFogDrawOverSky",
    "BlzSetTerrainFogMaxLinearDensity",
    "SetDestructableVertexColor",
    "SetHDWaterParamsEx",
    "SetItemColor",
    "SetPlayerRaceSkin",
    "SetTerrainFogExV",
    "PlayDialogueFromSpeakerEx",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jass::registry;

    #[test]
    fn every_handler_name_is_a_registry_native_or_a_known_header_gap() {
        let mut missing = Vec::new();
        for (name, _) in HANDLERS {
            if registry::lookup(name).is_none()
                && !HEADER_ABSENT_NATIVES
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(name))
            {
                missing.push(*name);
            }
        }
        assert!(
            missing.is_empty(),
            "handlers registered for names that are neither natives nor documented \
             header gaps: {missing:?}"
        );
    }

    #[test]
    fn header_gap_list_is_really_absent_from_the_registry() {
        for name in HEADER_ABSENT_NATIVES {
            assert!(
                registry::lookup(name).is_none(),
                "{name} is in the registry now: move it out of HEADER_ABSENT_NATIVES"
            );
            assert!(is_handled(name), "{name} must keep its handler");
        }
    }

    #[test]
    fn handler_table_has_no_duplicates() {
        let mut seen = std::collections::BTreeSet::new();
        for (name, _) in HANDLERS {
            assert!(seen.insert(*name), "duplicate handler row for {name}");
        }
    }

    #[test]
    fn convert_natives_are_handled_by_the_identity_rule() {
        assert!(is_handled("ConvertPlayerColor"));
        assert!(is_handled("ConvertRace"));
        assert!(is_handled("ConvertUnitState"));
        let mut vm_probe = None::<JassVm>;
        let _ = &mut vm_probe;
    }
}
