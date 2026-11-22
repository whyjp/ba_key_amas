package main

import "image"

// Collection stages
//
//	1. fixed meta   - identity, role/position/attack/armor/weapon class, base
//	                  terrain, equipment slot types, growth materials. These only
//	                  change with game patches, so they come from the dictionary
//	                  (cmd/namedict, refreshed on a patch) and are NOT scanned.
//	2. variable     - level, exp, stars, skills, bond, weapon, equipment, stats,
//	                  pin, notification dots, menu pass. Scanned every run.
//	3. single update - `--detail-current` re-reads the open profile only.
//
// `--verify-fixed` additionally reads the fixed fields from pixels and warns on
// any disagreement with the dictionary: use it after a patch or to audit the
// dictionary, not for routine scans.

// Options selects what extract reads.
type Options struct {
	VerifyFixed bool
}

// FixedMeta is the patch-bound data attached from the dictionary.
type FixedMeta struct {
	Source             string   `json:"source"` // "dictionary" or "pixels+dictionary"
	DictionaryID       int      `json:"dictionary_id,omitempty"`
	Ambiguous          bool     `json:"ambiguous,omitempty"` // several dictionary entries share the name
	EquipmentSlots     []string `json:"equipment_slots,omitempty"`
	PotentialMaterial  string   `json:"potential_material,omitempty"` // 능력개방 (not verified: no unlocked account sample)
	SkillMaterials     []string `json:"skill_materials,omitempty"`
	WeaponTerrain      string   `json:"weapon_terrain,omitempty"`
	WeaponTerrainBonus int      `json:"weapon_terrain_bonus,omitempty"`
	TerrainBase        Terrain  `json:"terrain_base"` // dictionary grades, before any weapon bonus
}

// pickEntry finds the dictionary entry for a resolved name. For names that exist
// twice (호시노(무장)) the cheap role-icon hint chooses; if that cannot decide,
// ok is true but ambiguous is set and the first entry is NOT trusted by callers.
func (r *NameResolver) pickEntry(name, roleHint string) (e dictEntry, ambiguous, ok bool) {
	var cands []dictEntry
	for _, c := range r.entries {
		if c.Name == name {
			cands = append(cands, c)
		}
	}
	switch len(cands) {
	case 0:
		return e, false, false
	case 1:
		return cands[0], false, true
	}
	var fit []dictEntry
	for _, c := range cands {
		if roleHint != "" && c.TacticRole == roleHint {
			fit = append(fit, c)
		}
	}
	if len(fit) == 1 {
		return fit[0], false, true
	}
	return cands[0], true, true
}

// applyFixed fills the fixed groups from the dictionary instead of pixels.
func applyFixed(s *Student, img image.Image) {
	e, amb, ok := defaultResolver.pickEntry(s.Name, roleFromIcon(img))
	if !ok {
		return
	}
	s.Fixed = &FixedMeta{
		Source: "dictionary", DictionaryID: e.ID, Ambiguous: amb,
		EquipmentSlots: e.EquipmentSlots, PotentialMaterial: e.PotentialMaterial, SkillMaterials: e.SkillMaterials,
		WeaponTerrain: e.WeaponTerrain, WeaponTerrainBonus: e.WeaponTerrainBonus,
		TerrainBase: Terrain{Urban: intp(e.Urban), Outdoor: intp(e.Outdoor), Indoor: intp(e.Indoor)},
	}
	if amb {
		s.Warnings = append(s.Warnings, "fixed_meta_ambiguous: several dictionary entries share this name")
		return // no fixed loadout rather than a possibly wrong one
	}
	s.Loadout = Loadout{TacticRole: e.TacticRole, Position: e.Position, AttackType: e.AttackType, ArmorType: e.ArmorType, WeaponClass: e.WeaponType}
}
