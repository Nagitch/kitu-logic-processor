//! Fixed-order combat rules; host I/O and device sampling stay outside this module.
use super::{
    json_message, ArenaState, Controls, Effect, Enemy, Grenade, Projectile, RunResult, Vec2,
};
use kitu_osc_ir::OscBundle;
use serde_json::json;

// Preserve the pinned C# hit cutoff; the nearest standard constant is different.
#[allow(clippy::approx_constant)]
const BLADE_ARC_DOT: f32 = 0.7071067;

#[derive(Default)]
struct DamageQueue {
    // First-hit insertion order matches the reference dictionary; deaths traverse
    // the enemy list backwards, independently of this damage-application order.
    enemies: Vec<(usize, i32, Vec<i32>)>,
    player: Vec<(i32, i32)>,
}
impl DamageQueue {
    fn enemy(&mut self, index: usize, damage: i32, attack: i32) {
        if let Some((_, total, attacks)) = self.enemies.iter_mut().find(|(i, _, _)| *i == index) {
            *total += damage;
            attacks.push(attack);
        } else {
            self.enemies.push((index, damage, vec![attack]));
        }
    }
}

fn emit(output: &mut OscBundle, tick: i64, address: &str, mut value: serde_json::Value) {
    value["tick"] = tick.into();
    value["order"] = output.messages.len().into();
    output.push(json_message(address, &value));
}

impl ArenaState {
    pub(super) fn step(
        &mut self,
        controls: Controls,
        uses: [bool; 2],
        dt: f32,
        tick: i64,
        output: &mut OscBundle,
    ) {
        if dt <= 0.0 || !dt.is_finite() || self.phase == 0 || self.phase == 5 {
            return;
        }
        if self.phase == 2 {
            self.transition_remaining -= dt;
            if self.transition_remaining <= 0.00001 {
                self.enter_first_floor(tick, output);
            }
            return;
        }
        self.elapsed += dt;
        for i in (0..self.effects.len()).rev() {
            self.effects[i].remaining -= dt;
            if self.effects[i].remaining <= 0.0 {
                let id = self.effects.remove(i).id;
                self.despawn("effect", id, tick, output);
            }
        }
        for cooldown in &mut self.weapon_cooldowns {
            *cooldown = (*cooldown - dt).max(0.0);
        }
        self.player_position = (self.player_position
            + controls.movement.clamp_length(1.0) * (5.0 * dt))
            .clamp_arena(0.5);
        if controls.has_aim {
            let direction = controls.aim - self.player_position;
            if direction.squared() > 0.000001 {
                self.aim_direction = direction.normalized();
            }
        }
        let mut damage = DamageQueue::default();
        for (index, pressed) in uses.into_iter().enumerate() {
            if pressed {
                self.use_item(index + 2, controls, tick, output);
            }
        }
        if controls.fire_a {
            self.use_weapon(0, &mut damage, tick, output);
        }
        if controls.fire_b {
            self.use_weapon(1, &mut damage, tick, output);
        }
        if self.phase == 3 {
            for index in 0..self.enemies.len() {
                self.update_enemy(index, dt, &mut damage, tick, output);
            }
        }
        self.update_projectiles(dt, &mut damage, tick, output);
        self.update_grenades(dt, &mut damage, tick, output);
        self.resolve_damage(damage, tick, output);
        self.inventory.advance_time(dt);
        if self.inventory.health <= 0 {
            emit(
                output,
                tick,
                "/game/arena/death",
                json!({"entityId":0,"kind":"player"}),
            );
            self.phase_to(5, tick, output);
            self.result = RunResult {
                present: true,
                floor: self.floor,
                elapsed: self.elapsed,
                enemies_defeated: self.enemies_defeated,
                bosses_defeated: self.bosses_defeated,
                floors_cleared: self.floors_cleared,
                max_health: self.inventory.max_health,
                attack_multiplier: self.inventory.attack_multiplier,
                equipment_names: self
                    .inventory
                    .equipment
                    .iter()
                    .map(|item| {
                        if item.id == 0 {
                            "Empty".into()
                        } else {
                            item.name.clone()
                        }
                    })
                    .collect(),
            };
            self.clear_transient(tick, output);
            emit(
                output,
                tick,
                "/game/arena/result",
                json!({"result":self.result}),
            );
            return;
        }
        if self.phase == 3 && self.enemies.is_empty() {
            self.floors_cleared += 1;
            self.phase_to(4, tick, output);
            self.clear_transient(tick, output);
            self.portal_armed = (self.player_position - Vec2 { x: 0.0, y: 7.5 }).length() > 1.25;
        }
        if self.portal_available {
            let overlaps = (self.player_position - Vec2 { x: 0.0, y: 7.5 }).length() <= 1.25;
            if !overlaps {
                self.portal_armed = true;
            } else if self.portal_armed
                && self.floor == 0
                && self.inventory.equipment[..2]
                    .iter()
                    .any(|item| item.is_weapon())
            {
                // Stage 4 opens the first combat floor. Endless progression and
                // repeating boss rewards are enabled by the following migration.
                self.transition_remaining = 0.3;
                self.portal_armed = false;
                self.inventory.chest.clear();
                self.enemies.clear();
                self.phase_to(2, tick, output);
                self.clear_transient(tick, output);
            }
        }
    }

    fn phase_to(&mut self, phase: i32, tick: i64, output: &mut OscBundle) {
        let previous = self.phase;
        self.phase = phase;
        self.portal_available = phase == 1 || phase == 4;
        self.chest_available = phase == 1 || (phase == 4 && self.floor % 5 == 0);
        self.emit_phase(previous, tick, output);
    }

    pub(super) fn emit_phase(&self, previous: i32, tick: i64, output: &mut OscBundle) {
        emit(
            output,
            tick,
            "/game/arena/phase",
            json!({"previous":previous,"phase":self.phase,"floor":self.floor,
            "floorsCleared":self.floors_cleared,"enemiesDefeated":self.enemies_defeated,"bossesDefeated":self.bosses_defeated}),
        );
    }

    fn enter_first_floor(&mut self, tick: i64, output: &mut OscBundle) {
        self.floor += 1;
        self.player_position = Vec2 { x: 0.0, y: -7.0 };
        self.aim_direction = Vec2 { x: 0.0, y: 1.0 };
        self.weapon_cooldowns = [0.0; 2];
        self.enemies.clear();
        for x in [-7.5, -4.5, -1.5] {
            let enemy = self.create_enemy(0, Vec2 { x, y: 6.0 });
            emit(
                output,
                tick,
                "/render/arena/spawn",
                json!({"kind":"enemy","entityId":enemy.id,"state":enemy}),
            );
            self.enemies.push(enemy);
        }
        self.phase_to(3, tick, output);
    }

    fn create_enemy(&mut self, kind: i32, position: Vec2) -> Enemy {
        let id = self.next_entity_id;
        self.next_entity_id += 1;
        let (hp, damage, speed, range, interval, radius) = match kind {
            1 => (30, 8, 2.0, 6.0, 1.5, 0.5),
            2 => (100, 20, 1.2, 1.8, 1.5, 0.7),
            3 => (300, 20, 1.5, 2.0, 1.5, 1.0),
            _ => (40, 10, 2.5, 1.5, 1.0, 0.5),
        };
        let max_health = (hp as f32 * (1.0 + 0.12 * (self.floor - 1) as f32)).ceil() as i32;
        Enemy {
            id,
            kind,
            position,
            health: max_health,
            max_health,
            radius,
            speed,
            damage: self.enemy_damage(damage),
            attack_interval: interval,
            attack_range: range,
            attack_cooldown: interval,
            boss_state: 0,
            phase_remaining: if kind == 3 { 3.0 } else { 0.0 },
        }
    }

    fn enemy_damage(&self, damage: i32) -> i32 {
        ((damage as f32 * (1.0 + 0.08 * (self.floor - 1) as f32)).floor() as i32).max(1)
    }
    fn player_damage(&self, damage: i32) -> i32 {
        ((damage as f32 * self.inventory.attack_multiplier).floor() as i32).max(1)
    }

    fn use_weapon(
        &mut self,
        slot: usize,
        pending: &mut DamageQueue,
        tick: i64,
        output: &mut OscBundle,
    ) {
        let weapon = self.inventory.equipment[slot].clone();
        if !weapon.is_weapon() || self.weapon_cooldowns[slot] > 0.00001 {
            return;
        }
        self.weapon_cooldowns[slot] = weapon.interval;
        let damage = self.player_damage(weapon.damage);
        let attack = self.next_entity_id;
        emit(
            output,
            tick,
            "/game/arena/attack",
            json!({"actorId":0,"attackId":attack,"itemId":weapon.id,
            "kind":weapon.kind,"origin":self.player_position,"direction":self.aim_direction}),
        );
        if weapon.kind == 0 {
            self.effect(
                0,
                self.player_position,
                self.aim_direction,
                2.0,
                0.15,
                tick,
                output,
            );
            for (index, enemy) in self.enemies.iter().enumerate() {
                let offset = enemy.position - self.player_position;
                if enemy.health > 0
                    && offset.length() <= 2.0 + enemy.radius
                    && (offset.squared() < 0.000001
                        || offset.normalized().dot(self.aim_direction) >= BLADE_ARC_DOT)
                {
                    pending.enemy(index, damage, attack);
                }
            }
        } else {
            let heavy = weapon.kind == 2;
            self.projectile(
                self.player_position,
                self.aim_direction,
                if heavy { 7.0 } else { 12.0 },
                if heavy { 10.0 } else { 12.0 },
                damage,
                false,
                tick,
                output,
            );
        }
    }

    fn use_item(&mut self, slot: usize, controls: Controls, tick: i64, output: &mut OscBundle) {
        let item = self.inventory.equipment[slot].clone();
        let mut target = self.player_position;
        let reason = if item.id == 0 {
            "empty_slot"
        } else if item.kind == 5 {
            "automatic_shield"
        } else if item.kind == 3 && self.inventory.health >= self.inventory.max_health {
            "full_health"
        } else if item.kind == 4
            && (!controls.has_aim || (controls.aim - self.player_position).squared() <= 0.000001)
        {
            "invalid_aim"
        } else {
            "ok"
        };
        if reason != "ok" {
            emit(
                output,
                tick,
                "/ui/arena/use",
                json!({"slot":slot,"itemId":item.id,"consumed":false,"code":reason}),
            );
            return;
        }
        if item.kind == 4 {
            target = (self.player_position
                + (controls.aim - self.player_position).clamp_length(8.0))
            .clamp_arena(0.1);
        }
        let Some(used) = self.inventory.use_item(slot as i32) else {
            return;
        };
        emit(
            output,
            tick,
            "/game/arena/inventory",
            json!({"operation":"/input/arena/use","slot":slot,"itemId":used.id,"inventory":self.inventory}),
        );
        emit(
            output,
            tick,
            "/ui/arena/use",
            json!({"slot":slot,"itemId":used.id,"consumed":true,"code":"ok"}),
        );
        if used.kind == 4 {
            let grenade = Grenade {
                id: self.next_entity_id,
                start: self.player_position,
                position: self.player_position,
                target,
                remaining: 0.5,
                damage: self.player_damage(100),
            };
            self.next_entity_id += 1;
            emit(
                output,
                tick,
                "/game/arena/attack",
                json!({"actorId":0,"attackId":grenade.id,"itemId":used.id,"kind":"grenade","origin":grenade.start,"target":target}),
            );
            emit(
                output,
                tick,
                "/render/arena/spawn",
                json!({"kind":"grenade","entityId":grenade.id,"state":grenade}),
            );
            self.grenades.push(grenade);
        }
    }

    fn update_enemy(
        &mut self,
        index: usize,
        dt: f32,
        pending: &mut DamageQueue,
        tick: i64,
        output: &mut OscBundle,
    ) {
        let mut enemy = self.enemies[index];
        if enemy.health <= 0 {
            return;
        }
        enemy.attack_cooldown = (enemy.attack_cooldown - dt).max(0.0);
        if enemy.kind == 3 {
            enemy.phase_remaining -= dt;
            if enemy.boss_state == 0 && enemy.phase_remaining <= 0.00001 {
                enemy.boss_state = 1;
                enemy.phase_remaining = 0.8;
                self.enemies[index] = enemy;
                return;
            }
            if enemy.boss_state == 1 {
                if enemy.phase_remaining <= 0.00001 {
                    emit(
                        output,
                        tick,
                        "/game/arena/attack",
                        json!({"actorId":enemy.id,"attackId":self.next_entity_id,"kind":"boss_burst","origin":enemy.position}),
                    );
                    for i in 0..8 {
                        let angle = i as f32 * std::f32::consts::PI / 4.0;
                        self.projectile(
                            enemy.position,
                            Vec2 {
                                x: angle.cos(),
                                y: angle.sin(),
                            },
                            5.0,
                            20.0,
                            self.enemy_damage(12),
                            true,
                            tick,
                            output,
                        );
                    }
                    enemy.boss_state = 2;
                    enemy.phase_remaining = 1.0;
                }
                self.enemies[index] = enemy;
                return;
            }
            if enemy.boss_state == 2 {
                if enemy.phase_remaining <= 0.00001 {
                    enemy.boss_state = 0;
                    enemy.phase_remaining = 3.0;
                }
                self.enemies[index] = enemy;
                return;
            }
        }
        let offset = self.player_position - enemy.position;
        let distance = offset.length();
        if distance > enemy.attack_range + 0.0001 {
            let travel = (enemy.speed * dt).min(distance - enemy.attack_range);
            enemy.position =
                (enemy.position + offset.normalized() * travel).clamp_arena(enemy.radius);
        } else if enemy.attack_cooldown <= 0.00001 {
            enemy.attack_cooldown = enemy.attack_interval;
            let attack = self.next_entity_id;
            emit(
                output,
                tick,
                "/game/arena/attack",
                json!({"actorId":enemy.id,"attackId":attack,"kind":enemy.kind,"origin":enemy.position,"direction":offset.normalized()}),
            );
            if enemy.kind == 1 {
                self.projectile(
                    enemy.position,
                    if distance <= 0.00001 {
                        Vec2 { x: 0.0, y: 1.0 }
                    } else {
                        offset.normalized()
                    },
                    6.0,
                    12.0,
                    enemy.damage,
                    true,
                    tick,
                    output,
                );
            } else {
                pending.player.push((enemy.damage, attack));
                self.effect(
                    2,
                    self.player_position,
                    offset.normalized(),
                    0.5,
                    0.15,
                    tick,
                    output,
                );
            }
        }
        self.enemies[index] = enemy;
    }

    #[allow(clippy::too_many_arguments)]
    fn projectile(
        &mut self,
        position: Vec2,
        direction: Vec2,
        speed: f32,
        distance: f32,
        damage: i32,
        enemy_owned: bool,
        tick: i64,
        output: &mut OscBundle,
    ) {
        let shot = Projectile {
            id: self.next_entity_id,
            position,
            direction,
            speed,
            distance_remaining: distance,
            damage,
            enemy_owned,
            radius: 0.14,
        };
        self.next_entity_id += 1;
        emit(
            output,
            tick,
            "/render/arena/spawn",
            json!({"kind":"projectile","entityId":shot.id,"state":shot}),
        );
        self.projectiles.push(shot);
    }

    fn update_projectiles(
        &mut self,
        dt: f32,
        pending: &mut DamageQueue,
        tick: i64,
        output: &mut OscBundle,
    ) {
        for i in (0..self.projectiles.len()).rev() {
            let mut shot = self.projectiles[i];
            let from = shot.position;
            let mut travel = (shot.speed * dt).min(shot.distance_remaining);
            let wall = distance_to_wall(from, shot.direction, shot.radius);
            let hits_wall = wall <= travel;
            travel = travel.min(wall);
            let to = from + shot.direction * travel;
            let mut closest = f32::INFINITY;
            let mut closest_enemy = 0;
            if shot.enemy_owned {
                if self.phase == 3 {
                    if let Some(hit) =
                        segment_circle(from, to, self.player_position, 0.5 + shot.radius)
                    {
                        closest = hit;
                    }
                }
            } else {
                for (index, enemy) in self.enemies.iter().enumerate() {
                    if enemy.health <= 0 {
                        continue;
                    }
                    if let Some(hit) =
                        segment_circle(from, to, enemy.position, enemy.radius + shot.radius)
                    {
                        if hit < closest {
                            closest = hit;
                            closest_enemy = index;
                        }
                    }
                }
            }
            if closest != f32::INFINITY {
                if shot.enemy_owned {
                    pending.player.push((shot.damage, shot.id));
                } else {
                    pending.enemy(closest_enemy, shot.damage, shot.id);
                }
                self.effect(
                    2,
                    from.lerp(to, closest),
                    shot.direction,
                    0.35,
                    0.12,
                    tick,
                    output,
                );
                self.projectiles.remove(i);
                self.despawn("projectile", shot.id, tick, output);
                continue;
            }
            shot.position = to;
            shot.distance_remaining -= travel;
            if hits_wall || shot.distance_remaining <= 0.00001 {
                self.projectiles.remove(i);
                self.despawn("projectile", shot.id, tick, output);
            } else {
                self.projectiles[i] = shot;
            }
        }
    }

    fn update_grenades(
        &mut self,
        dt: f32,
        pending: &mut DamageQueue,
        tick: i64,
        output: &mut OscBundle,
    ) {
        for i in (0..self.grenades.len()).rev() {
            let mut grenade = self.grenades[i];
            grenade.remaining -= dt;
            grenade.position = grenade.start.lerp(
                grenade.target,
                (1.0 - grenade.remaining / 0.5).clamp(0.0, 1.0),
            );
            if grenade.remaining > 0.00001 {
                self.grenades[i] = grenade;
                continue;
            }
            for (index, enemy) in self.enemies.iter().enumerate() {
                if enemy.health > 0
                    && (grenade.target - enemy.position).length() <= 3.0 + enemy.radius
                {
                    pending.enemy(index, grenade.damage, grenade.id);
                }
            }
            self.effect(
                1,
                grenade.target,
                Vec2 { x: 0.0, y: 1.0 },
                3.0,
                0.3,
                tick,
                output,
            );
            self.grenades.remove(i);
            self.despawn("grenade", grenade.id, tick, output);
        }
    }

    fn resolve_damage(&mut self, damage: DamageQueue, tick: i64, output: &mut OscBundle) {
        for (index, amount, attacks) in damage.enemies {
            let enemy = &mut self.enemies[index];
            enemy.health = (enemy.health - amount).max(0);
            emit(
                output,
                tick,
                "/game/arena/damage",
                json!({"targetId":enemy.id,"targetKind":"enemy","damage":amount,"attackIds":attacks,"health":enemy.health,"absorbed":0}),
            );
        }
        for i in (0..self.enemies.len()).rev() {
            if self.enemies[i].health > 0 {
                continue;
            }
            let enemy = self.enemies.remove(i);
            self.enemies_defeated += 1;
            if enemy.kind == 3 {
                self.bosses_defeated += 1;
            }
            emit(
                output,
                tick,
                "/game/arena/death",
                json!({"entityId":enemy.id,"kind":enemy.kind}),
            );
            self.despawn("enemy", enemy.id, tick, output);
        }
        for (amount, attack) in damage.player {
            let shields_before: i32 = self.inventory.equipment[2..4]
                .iter()
                .filter(|item| item.kind == 5)
                .map(|item| item.shield)
                .sum();
            self.inventory.apply_damage(amount);
            let shields_after: i32 = self.inventory.equipment[2..4]
                .iter()
                .filter(|item| item.kind == 5)
                .map(|item| item.shield)
                .sum();
            emit(
                output,
                tick,
                "/game/arena/damage",
                json!({"targetId":0,"targetKind":"player","damage":amount,"attackIds":[attack],"health":self.inventory.health,"absorbed":shields_before-shields_after}),
            );
        }
    }

    fn clear_transient(&mut self, tick: i64, output: &mut OscBundle) {
        for shot in &self.projectiles {
            self.despawn("projectile", shot.id, tick, output);
        }
        for grenade in &self.grenades {
            self.despawn("grenade", grenade.id, tick, output);
        }
        for effect in &self.effects {
            self.despawn("effect", effect.id, tick, output);
        }
        self.projectiles.clear();
        self.grenades.clear();
        self.effects.clear();
    }
    fn despawn(&self, kind: &str, id: i32, tick: i64, output: &mut OscBundle) {
        emit(
            output,
            tick,
            "/render/arena/despawn",
            json!({"kind":kind,"entityId":id}),
        );
    }
    #[allow(clippy::too_many_arguments)]
    fn effect(
        &mut self,
        kind: i32,
        position: Vec2,
        direction: Vec2,
        radius: f32,
        duration: f32,
        tick: i64,
        output: &mut OscBundle,
    ) {
        let effect = Effect {
            id: self.next_entity_id,
            kind,
            position,
            direction,
            radius,
            remaining: duration,
        };
        self.next_entity_id += 1;
        emit(
            output,
            tick,
            "/render/arena/spawn",
            json!({"kind":"effect","entityId":effect.id,"state":effect}),
        );
        self.effects.push(effect);
    }
}

fn distance_to_wall(position: Vec2, direction: Vec2, radius: f32) -> f32 {
    let edge = 10.0 - radius;
    let mut distance = f32::INFINITY;
    if direction.x > 0.0 {
        distance = distance.min((edge - position.x) / direction.x);
    }
    if direction.x < 0.0 {
        distance = distance.min((-edge - position.x) / direction.x);
    }
    if direction.y > 0.0 {
        distance = distance.min((edge - position.y) / direction.y);
    }
    if direction.y < 0.0 {
        distance = distance.min((-edge - position.y) / direction.y);
    }
    distance.max(0.0)
}
fn segment_circle(start: Vec2, end: Vec2, center: Vec2, radius: f32) -> Option<f32> {
    let relative = start - center;
    let c = relative.squared() - radius * radius;
    if c <= 0.0 {
        return Some(0.0);
    }
    let segment = end - start;
    let a = segment.squared();
    if a <= 0.0000001 {
        return None;
    }
    let b = relative.dot(segment);
    let discriminant = b * b - a * c;
    if discriminant < 0.0 {
        return None;
    }
    let hit = (-b - discriminant.sqrt()) / a;
    if (0.0..=1.0).contains(&hit) {
        Some(hit)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    // Arrangements and expected values are taken from the unchanged C#
    // ArenaSimulationTests. These mutate test setup only, never network state.
    use super::*;
    use crate::arena::inventory::Item;
    const DT: f32 = 1.0 / 60.0;
    fn state(floor: i32) -> ArenaState {
        let mut state = ArenaState {
            phase: 3,
            floor,
            floors_cleared: floor - 1,
            ..ArenaState::default()
        };
        for x in [-7.5, -4.5, -1.5] {
            let enemy = state.create_enemy(0, Vec2 { x, y: 6.0 });
            state.enemies.push(enemy);
        }
        state
    }
    fn freeze(enemy: &mut Enemy, position: Vec2, hp: i32) {
        enemy.position = position;
        enemy.health = hp;
        enemy.max_health = hp;
        enemy.speed = 0.0;
        enemy.attack_cooldown = 100.0;
    }
    fn item(id: i32, kind: i32, damage: i32, interval: f32) -> Item {
        Item {
            id,
            kind,
            damage,
            interval,
            name: format!("test-{id}"),
            ..Item::default()
        }
    }
    fn step(state: &mut ArenaState, dt: f32, controls: Controls, uses: [bool; 2]) -> OscBundle {
        let mut output = OscBundle::new();
        state.step(controls, uses, dt, 0, &mut output);
        output
    }
    fn aimed(point: Vec2) -> Controls {
        Controls {
            has_aim: true,
            aim: point,
            ..Controls::default()
        }
    }
    #[test]
    fn simultaneous_last_enemy_and_player_death_records_kill_before_result_and_never_clear() {
        for floor in [1, 5] {
            let mut state = state(floor);
            state.enemies.truncate(1);
            let enemy = &mut state.enemies[0];
            enemy.kind = if floor == 5 { 3 } else { 0 };
            enemy.position = Vec2 { x: 0.0, y: 1.0 };
            enemy.health = 20;
            enemy.damage = 1000;
            enemy.attack_cooldown = 0.0;
            enemy.boss_state = 0;
            enemy.phase_remaining = 3.0;
            let output = step(
                &mut state,
                DT,
                Controls {
                    fire_a: true,
                    ..aimed(Vec2 { x: 0.0, y: 1.0 })
                },
                [false; 2],
            );
            assert_eq!(state.phase, 5);
            assert_eq!(state.inventory.health, 0);
            assert_eq!(state.enemies_defeated, 1);
            assert_eq!(state.bosses_defeated, if floor == 5 { 1 } else { 0 });
            assert_eq!(state.floors_cleared, floor - 1);
            assert!(
                !state.portal_available
                    && !state.chest_available
                    && state.inventory.chest.is_empty()
            );
            let game: Vec<_> = output
                .messages
                .iter()
                .filter(|m| m.address.starts_with("/game/arena/"))
                .map(|m| m.address.as_str())
                .collect();
            assert_eq!(
                game,
                vec![
                    "/game/arena/attack",
                    "/game/arena/attack",
                    "/game/arena/damage",
                    "/game/arena/death",
                    "/game/arena/damage",
                    "/game/arena/death",
                    "/game/arena/phase",
                    "/game/arena/result"
                ]
            );
            let result = state.result.clone();
            let elapsed = state.elapsed;
            step(
                &mut state,
                10.0,
                Controls {
                    fire_a: true,
                    movement: Vec2 { x: 0.0, y: 1.0 },
                    ..Controls::default()
                },
                [true; 2],
            );
            assert_eq!(state.result, result);
            assert_eq!(state.elapsed, elapsed);
            assert!(state.projectiles.is_empty());
        }
    }
    #[test]
    fn independent_blades_hit_front_cone_once_and_keep_separate_cooldowns() {
        let mut state = state(1);
        freeze(&mut state.enemies[0], Vec2 { x: 0.0, y: 1.0 }, 100);
        freeze(&mut state.enemies[1], Vec2 { x: 0.0, y: -1.0 }, 100);
        freeze(&mut state.enemies[2], Vec2 { x: 3.0, y: 0.0 }, 100);
        state.inventory.equipment[0] = item(9000, 0, 20, 0.5);
        state.inventory.equipment[1] = item(9001, 0, 30, 1.0);
        let input = Controls {
            fire_a: true,
            fire_b: true,
            ..aimed(Vec2 { x: 0.0, y: 5.0 })
        };
        step(&mut state, DT, input, [false; 2]);
        assert_eq!(
            state.enemies.iter().map(|e| e.health).collect::<Vec<_>>(),
            vec![50, 100, 100]
        );
        step(&mut state, DT, input, [false; 2]);
        assert_eq!(state.enemies[0].health, 50);
        for _ in 0..29 {
            step(&mut state, DT, input, [false; 2]);
        }
        assert_eq!(state.enemies[0].health, 30);
    }
    #[test]
    fn projectile_sweep_selects_nearest_target_and_hits_initial_overlap() {
        let mut state = state(1);
        freeze(&mut state.enemies[0], Vec2 { x: 0.0, y: 5.0 }, 100);
        freeze(&mut state.enemies[1], Vec2 { x: 0.0, y: 1.0 }, 100);
        freeze(&mut state.enemies[2], Vec2 { x: 8.0, y: 0.0 }, 100);
        state.inventory.equipment[0] = item(9000, 1, 12, 0.25);
        step(
            &mut state,
            0.5,
            Controls {
                fire_a: true,
                ..aimed(Vec2 { x: 0.0, y: 9.0 })
            },
            [false; 2],
        );
        assert_eq!(state.enemies[1].health, 88);
        assert_eq!(state.enemies[0].health, 100);
        assert!(state.projectiles.is_empty());
        state.enemies.truncate(1);
        freeze(&mut state.enemies[0], Vec2::default(), 100);
        state.weapon_cooldowns = [0.0; 2];
        step(
            &mut state,
            DT,
            Controls {
                fire_a: true,
                ..Controls::default()
            },
            [false; 2],
        );
        assert_eq!(state.enemies[0].health, 88);
        assert!(state.projectiles.is_empty());
    }
    #[test]
    fn hostile_sweeps_ignore_enemies_and_projectiles_expire_at_walls_or_range() {
        let mut state = state(1);
        for enemy in &mut state.enemies {
            freeze(enemy, Vec2 { x: 0.0, y: -3.0 }, 100);
        }
        state.projectiles.push(Projectile {
            id: 10000,
            position: Vec2 { x: 0.0, y: -5.0 },
            direction: Vec2 { x: 0.0, y: 1.0 },
            speed: 20.0,
            distance_remaining: 20.0,
            damage: 8,
            enemy_owned: true,
            ..Projectile::default()
        });
        step(&mut state, 0.5, Controls::default(), [false; 2]);
        assert_eq!(state.inventory.health, 92);
        assert!(state.enemies.iter().all(|e| e.health == 100));
        assert!(state.projectiles.is_empty());
        state.phase = 1;
        state.enemies.clear();
        state.player_position = Vec2 { x: 9.0, y: 0.0 };
        state.inventory.equipment[0] = item(9000, 1, 12, 0.25);
        step(
            &mut state,
            DT,
            Controls {
                fire_a: true,
                ..aimed(Vec2 { x: 20.0, y: 0.0 })
            },
            [false; 2],
        );
        assert_eq!(state.projectiles.len(), 1);
        step(&mut state, 0.2, Controls::default(), [false; 2]);
        assert!(state.projectiles.is_empty());
        state.projectiles.push(Projectile {
            id: 10000,
            direction: Vec2 { x: 0.0, y: 1.0 },
            speed: 12.0,
            distance_remaining: 1.0,
            damage: 1,
            ..Projectile::default()
        });
        step(&mut state, 0.2, Controls::default(), [false; 2]);
        assert!(state.projectiles.is_empty());
    }
    #[test]
    fn grenade_needs_nonzero_aim_and_keeps_throw_damage_across_a_later_upgrade() {
        let mut state = state(1);
        freeze(&mut state.enemies[0], Vec2 { x: 0.0, y: 4.0 }, 300);
        freeze(&mut state.enemies[1], Vec2 { x: 2.0, y: 4.0 }, 300);
        freeze(&mut state.enemies[2], Vec2 { x: -8.0, y: -8.0 }, 300);
        state.inventory.equipment[2] = item(9000, 4, 0, 0.0);
        step(&mut state, DT, Controls::default(), [true, false]);
        assert_eq!(state.inventory.equipment[2].id, 9000);
        step(&mut state, DT, aimed(Vec2::default()), [true, false]);
        assert_eq!(state.inventory.equipment[2].id, 9000);
        step(
            &mut state,
            DT,
            aimed(Vec2 { x: 0.0, y: 4.0 }),
            [true, false],
        );
        assert_eq!(state.inventory.equipment[2].id, 0);
        assert_eq!(state.grenades.len(), 1);
        state.inventory.backpack[0] = item(9001, 7, 0, 0.0);
        assert!(state.inventory.use_upgrade(0));
        state.player_position = state.enemies[0].position;
        for _ in 0..28 {
            step(&mut state, DT, Controls::default(), [false; 2]);
        }
        assert_eq!(state.enemies[0].health, 300);
        step(&mut state, DT, Controls::default(), [false; 2]);
        assert_eq!(
            state.enemies.iter().map(|e| e.health).collect::<Vec<_>>(),
            vec![200, 200, 300]
        );
        assert_eq!(state.inventory.health, 100);
        assert!(state.grenades.is_empty());
        for _ in 0..30 {
            step(&mut state, DT, Controls::default(), [false; 2]);
        }
        assert_eq!(state.enemies[0].health, 200);
    }
    #[test]
    fn grenade_target_clamps_to_throw_range_and_arena_edge() {
        let mut state = ArenaState {
            phase: 1,
            ..ArenaState::default()
        };
        state.inventory.equipment[2] = item(9000, 4, 0, 0.0);
        step(
            &mut state,
            DT,
            aimed(Vec2 { x: 100.0, y: 0.0 }),
            [true, false],
        );
        assert_eq!(state.grenades[0].target.x, 8.0);
        state.player_position = Vec2 { x: 9.0, y: 0.0 };
        state.inventory.equipment[2] = item(9001, 4, 0, 0.0);
        step(
            &mut state,
            DT,
            aimed(Vec2 { x: 100.0, y: 0.0 }),
            [true, false],
        );
        assert!(state.grenades[1].target.x > 9.0 && state.grenades[1].target.x < 10.0);
    }
    #[test]
    fn medkit_a_precedes_b_and_incoming_damage() {
        let mut state = state(1);
        state.enemies.truncate(1);
        freeze(&mut state.enemies[0], Vec2 { x: 0.0, y: 1.0 }, 100);
        state.enemies[0].attack_cooldown = 0.0;
        state.inventory.apply_damage(50);
        state.inventory.equipment[2] = item(9000, 3, 0, 0.0);
        state.inventory.equipment[3] = item(9001, 3, 0, 0.0);
        step(&mut state, DT, Controls::default(), [true; 2]);
        assert_eq!(state.inventory.health, 90);
        assert_eq!(state.inventory.equipment[2].id, 0);
        assert_eq!(state.inventory.equipment[3].id, 9001);
    }
    #[test]
    fn boss_telegraph_and_recovery_suppress_melee_and_fire_eight_shots() {
        let mut state = state(5);
        let mut boss = state.create_enemy(3, Vec2::default());
        boss.speed = 0.0;
        boss.attack_cooldown = 0.0;
        boss.phase_remaining = DT;
        state.enemies = vec![boss];
        state.player_position = Vec2 { x: 1.0, y: 0.0 };
        step(&mut state, DT, Controls::default(), [false; 2]);
        assert_eq!(state.enemies[0].boss_state, 1);
        assert_eq!(state.inventory.health, 100);
        for _ in 0..47 {
            step(&mut state, DT, Controls::default(), [false; 2]);
        }
        assert!(state.projectiles.is_empty());
        step(&mut state, DT, Controls::default(), [false; 2]);
        assert_eq!(state.projectiles.len(), 8);
        assert_eq!(state.enemies[0].boss_state, 2);
        state.projectiles.clear();
        for _ in 0..59 {
            step(&mut state, DT, Controls::default(), [false; 2]);
        }
        assert_eq!(state.inventory.health, 100);
        step(&mut state, DT, Controls::default(), [false; 2]);
        assert_eq!(state.enemies[0].boss_state, 0);
        assert_eq!(state.inventory.health, 100);
        step(&mut state, DT, Controls::default(), [false; 2]);
        assert!(state.inventory.health < 100);
    }
    #[test]
    fn contact_waits_initial_interval_and_pursuers_eventually_kill_an_idle_player() {
        let mut state = state(1);
        state.enemies.truncate(1);
        state.enemies[0].position = state.player_position;
        step(&mut state, 0.5, Controls::default(), [false; 2]);
        assert_eq!(state.inventory.health, 100);
        step(&mut state, 0.5, Controls::default(), [false; 2]);
        assert_eq!(state.inventory.health, 90);
        for _ in 0..1800 {
            step(&mut state, DT, Controls::default(), [false; 2]);
            if state.phase == 5 {
                break;
            }
        }
        assert_eq!(state.phase, 5);
        assert_eq!(state.inventory.health, 0);
        assert_eq!(state.enemies_defeated, 0);
        assert_eq!(state.result.floor, 1);
    }
}
