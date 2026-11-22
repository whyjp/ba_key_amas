// namedict builds the Korean student dictionary used by baocr: spelling for OCR
// correction plus a *reference* block of community data (SchaleDB) that is only
// used to cross-check pixel-read values. It never fills pixel fields.
// bluearchive.wiki has only EN/JP names, so it cannot supply Korean spellings.
package main

import (
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"sort"
	"strings"
	"time"
)

const source = "https://schaledb.com/data/kr/students.min.json"
const itemsSource = "https://schaledb.com/data/kr/items.min.json"

type raw struct {
	ID                int      `json:"Id"`
	Name              string   `json:"Name"`
	SquadType         string   `json:"SquadType"`
	TacticRole        string   `json:"TacticRole"`
	Position          string   `json:"Position"`
	BulletType        string   `json:"BulletType"`
	ArmorType         string   `json:"ArmorType"`
	WeaponType        string   `json:"WeaponType"`
	StarGrade         int      `json:"StarGrade"`
	School            string   `json:"School"`
	Club              string   `json:"Club"`
	SchoolYear        string   `json:"SchoolYear"`
	CharacterAge      string   `json:"CharacterAge"`
	Birthday          string   `json:"Birthday"`
	Height            string   `json:"CharHeightMetric"`
	FamilyName        string   `json:"FamilyName"`
	PersonalName      string   `json:"PersonalName"`
	Hobby             string   `json:"Hobby"`
	Designer          string   `json:"Designer"`
	Illustrator       string   `json:"Illustrator"`
	Voice             string   `json:"CharacterVoice"`
	Equipment         []string `json:"Equipment"`
	PotentialMaterial int      `json:"PotentialMaterial"`
	SkillExMaterial   [][]int  `json:"SkillExMaterial"`
	SkillMaterial     [][]int  `json:"SkillMaterial"`
	Weapon            struct {
		AdaptationType  string `json:"AdaptationType"`
		AdaptationValue int    `json:"AdaptationValue"`
	} `json:"Weapon"`
	Street  int `json:"StreetBattleAdaptation"`
	Outdoor int `json:"OutdoorBattleAdaptation"`
	Indoor  int `json:"IndoorBattleAdaptation"`
}

type entry struct {
	ID   int    `json:"id"`
	Name string `json:"name"`
	// Base is the original student name; Outfit is the parenthesised costume.
	// A "*" suffix (e.g. 시로코*테러) is a different character, so it stays in Base.
	Base   string `json:"base"`
	Outfit string `json:"outfit,omitempty"`
	// Role is only a cross-check hint for pixel-read roles; it never fills them.
	Role string `json:"role"`
	// Display values are in-game Korean labels so they compare with OCR text.
	TacticRole  string `json:"tactic_role"`
	Position    string `json:"position"`
	AttackType  string `json:"attack_type"`
	ArmorType   string `json:"armor_type"`
	WeaponType  string `json:"weapon_type"`
	StarGrade   int    `json:"star_grade"`
	School      string `json:"school"`
	Club        string `json:"club"`
	SchoolYear  string `json:"school_year,omitempty"`
	Age         string `json:"age,omitempty"`
	Birthday    string `json:"birthday,omitempty"`
	Height      string `json:"height,omitempty"`
	FullName    string `json:"full_name,omitempty"`
	Hobby       string `json:"hobby,omitempty"`
	Designer    string `json:"designer,omitempty"`
	Illustrator string `json:"illustrator,omitempty"`
	Voice       string `json:"voice,omitempty"`
	// Fixed growth data: which materials/slots the student needs (never quantities).
	EquipmentSlots     []string `json:"equipment_slots,omitempty"`
	PotentialMaterial  string   `json:"potential_material,omitempty"` // 능력개방
	SkillMaterials     []string `json:"skill_materials,omitempty"`
	WeaponTerrain      string   `json:"weapon_terrain,omitempty"` // Urban/Outdoor/Indoor raised by the unique weapon
	WeaponTerrainBonus int      `json:"weapon_terrain_bonus,omitempty"`
	Urban              int      `json:"terrain_urban"`
	Outdoor            int      `json:"terrain_outdoor"`
	Indoor             int      `json:"terrain_indoor"`
}

type dict struct {
	Source  string  `json:"source"`
	Fetched string  `json:"fetched"`
	Names   []entry `json:"names"`
}

var (
	tactic   = map[string]string{"DamageDealer": "딜러", "Tanker": "탱커", "Healer": "힐러", "Supporter": "서포터", "Vehicle": "T.S"}
	position = map[string]string{"Front": "FRONT", "Middle": "MIDDLE", "Back": "BACK"}
	attack   = map[string]string{"Explosion": "폭발", "Pierce": "관통", "Mystic": "신비", "Sonic": "진동"}
	armor    = map[string]string{"LightArmor": "경장갑", "HeavyArmor": "중장갑", "Unarmed": "특수장갑", "ElasticArmor": "탄력장갑", "CompositeArmor": "복합장갑"}
)

// splitName: "리오(무장)" -> 리오 / 무장; "시로코*테러" stays whole.
func splitName(n string) (base, outfit string) {
	if i := strings.Index(n, "("); i > 0 && strings.HasSuffix(n, ")") {
		return n[:i], n[i+1 : len(n)-1]
	}
	return n, ""
}

func main() {
	out := "cmd/baocr/student_names_kr.json"
	if len(os.Args) > 1 {
		out = os.Args[1]
	}
	resp, err := http.Get(source)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	defer resp.Body.Close()
	var m map[string]raw
	if err := json.NewDecoder(resp.Body).Decode(&m); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	items := fetchItems()
	d := dict{Source: source, Fetched: time.Now().Format("2006-01-02")}
	for _, r := range m {
		role := ""
		switch r.SquadType {
		case "Main":
			role = "STRIKER"
		case "Support":
			role = "SPECIAL"
		}
		b, o := splitName(r.Name)
		d.Names = append(d.Names, entry{
			ID: r.ID, Name: r.Name, Base: b, Outfit: o, Role: role,
			TacticRole: tactic[r.TacticRole], Position: position[r.Position],
			AttackType: attack[r.BulletType], ArmorType: armor[r.ArmorType], WeaponType: r.WeaponType,
			StarGrade: r.StarGrade, School: r.School, Club: r.Club, SchoolYear: r.SchoolYear,
			Age: r.CharacterAge, Birthday: r.Birthday, Height: r.Height,
			FullName: strings.TrimSpace(r.FamilyName + " " + r.PersonalName), Hobby: r.Hobby, Designer: r.Designer, Illustrator: r.Illustrator, Voice: r.Voice,
			Urban: r.Street, Outdoor: r.Outdoor, Indoor: r.Indoor,
			EquipmentSlots: r.Equipment, PotentialMaterial: items[r.PotentialMaterial], SkillMaterials: skillMaterials(r, items),
			WeaponTerrain: r.Weapon.AdaptationType, WeaponTerrainBonus: r.Weapon.AdaptationValue,
		})
	}
	sort.Slice(d.Names, func(i, j int) bool { return d.Names[i].ID < d.Names[j].ID })
	b, _ := json.MarshalIndent(d, "", " ")
	if err := os.WriteFile(out, b, 0o644); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	fmt.Println("wrote", len(d.Names), "names to", out)
}

// fetchItems maps item id -> Korean item name (for material names).
func fetchItems() map[int]string {
	resp, err := http.Get(itemsSource)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	defer resp.Body.Close()
	var raw map[string]struct {
		Name string `json:"Name"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&raw); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	out := map[int]string{}
	for k, v := range raw {
		var id int
		if _, err := fmt.Sscanf(k, "%d", &id); err == nil {
			out[id] = v.Name
		}
	}
	return out
}

// skillMaterials lists the distinct materials any skill level needs.
func skillMaterials(r raw, items map[int]string) []string {
	seen := map[string]bool{}
	var out []string
	for _, group := range append(append([][]int{}, r.SkillExMaterial...), r.SkillMaterial...) {
		for _, id := range group {
			if n := items[id]; n != "" && !seen[n] {
				seen[n] = true
				out = append(out, n)
			}
		}
	}
	return out
}
