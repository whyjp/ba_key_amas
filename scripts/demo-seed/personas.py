"""Demo teachers: who they are and what they love. Pure data, read by build_demo.py.

Popularity. TOP10 is the result of the Dengeki Online character popularity poll of May 2026 (Japan, every
character was eligible): Hoshino, Hina, Mika, Kei, Kayoko, Yuuka, Arisu, Aru, Kazusa, Rio. WIDELY_LOVED is a
judgement (earlier Dengeki polls, tier lists and community talk), not a measured ranking.
Names are the Korean base names of the global release; an outfit variant counts as its base name.
"""

TOP10 = ["호시노", "히나", "미카", "케이", "카요코", "유우카", "아리스", "아루", "카즈사", "리오"]

WIDELY_LOVED = ["아즈사", "시로코", "노아", "하루나", "이오리", "와카모", "키사키", "마리", "세리나", "코하루",
                "네루", "미도리", "모모이", "유즈", "치나츠", "하스미", "아코", "카린", "토키", "사오리"]

# level: account level. owned: number of students. trained: how many of them are levelled up beyond level 1.
# love: base names the account is fond of (bond, pin, stars, weapon). schools / roles / outfits: tastes that
# raise the chance of owning a student. invest: 0..1, how far a trained student's skills and gear are raised.
# bond_top: bond ranks of the favourites, highest first (the game caps at 100, so only a few reach it).
# Codes are made from the index, see build_demo.code_of.
PERSONAS = [
    # newcomers
    dict(nick="데모 새내기 선생", level=18, owned=22, trained=5, love=["아리스"], invest=0.25, bond_top=[12, 8, 6], style="newcomer"),
    dict(nick="데모 갓 시작", level=26, owned=34, trained=8, love=["호시노"], invest=0.3, bond_top=[20, 14, 9], style="newcomer"),
    dict(nick="데모 뉴비 샬레", level=33, owned=44, trained=11, love=["유우카", "노아"], invest=0.35, bond_top=[26, 18, 12], style="newcomer"),
    # middle
    dict(nick="데모 중급 선생", level=48, owned=68, trained=19, love=["히나", "이오리"], invest=0.45, bond_top=[34, 26, 20, 14], style="middle"),
    dict(nick="데모 이야기 수집", level=52, owned=74, trained=14, love=["시로코", "노노미", "세리카"], invest=0.3, bond_top=[40, 30, 22], style="story"),
    dict(nick="데모 쿠키 선생", level=56, owned=82, trained=24, love=["미카", "세리나"], invest=0.5, bond_top=[48, 40, 32, 24, 18], style="middle"),
    dict(nick="데모 ㅇㅂㄷㅅ", level=60, owned=88, trained=26, love=["호시노", "시로코", "세리카", "노노미", "아야네"], schools=["Abydos"], invest=0.5, bond_top=[70, 52, 44, 38, 30], style="school"),
    # like the real account of the author: many students, few trained
    dict(nick="데모 수집가 69", level=69, owned=104, trained=31, love=["케이", "아리스"], invest=0.4, bond_top=[24, 18, 14, 11], style="collector"),
    dict(nick="데모 도감 완성중", level=66, owned=112, trained=22, love=[], outfits=2.5, invest=0.3, bond_top=[16, 12, 9], style="collector"),
    # fans of the top ten
    dict(nick="데모 호시노 올인", level=78, owned=118, trained=40, love=["호시노"], invest=0.7, bond_top=[100, 56, 44, 36], style="fan"),
    dict(nick="데모 히나 위원장", level=74, owned=96, trained=33, love=["히나", "아코", "이오리"], schools=["Gehenna"], invest=0.65, bond_top=[92, 60, 48, 36], style="fan"),
    dict(nick="데모 미카 공주님", level=80, owned=124, trained=38, love=["미카", "세리나", "나기사"], schools=["Trinity"], invest=0.7, bond_top=[88, 64, 50, 40], style="fan"),
    dict(nick="데모 케이 연구소", level=72, owned=92, trained=30, love=["케이", "리오", "아리스", "모모이", "미도리", "유즈"], schools=["Millennium"], invest=0.6, bond_top=[76, 58, 50, 42, 36], style="fan"),
    dict(nick="데모 카요코 비서", level=70, owned=90, trained=29, love=["카요코", "아루", "하루카", "무츠키"], schools=["Gehenna"], invest=0.6, bond_top=[84, 52, 44, 38, 30], style="fan"),
    dict(nick="데모 유우카 회계", level=76, owned=108, trained=36, love=["유우카", "노아", "리오"], schools=["Millennium"], invest=0.65, bond_top=[90, 62, 48, 36], style="fan"),
    dict(nick="데모 용사 아리스", level=64, owned=86, trained=24, love=["아리스", "모모이", "미도리", "유즈"], invest=0.55, bond_top=[80, 64, 50, 40, 30], style="fan"),
    dict(nick="데모 아루 사장님", level=68, owned=84, trained=25, love=["아루", "무츠키", "하루카", "카요코"], schools=["Gehenna"], invest=0.55, bond_top=[82, 48, 38, 30], style="fan"),
    dict(nick="데모 카즈사 간식", level=62, owned=80, trained=21, love=["카즈사", "나츠", "미카", "하나코"], invest=0.5, bond_top=[72, 44, 34, 26], style="fan"),
    dict(nick="데모 리오 회장님", level=82, owned=130, trained=42, love=["리오", "아리스", "유우카", "노아", "케이"], schools=["Millennium"], invest=0.75, bond_top=[100, 70, 54, 44, 36, 30], style="fan"),
    # tastes
    dict(nick="데모 힐러 전문", level=58, owned=70, trained=20, love=["세리나", "하나에", "마리"], roles=["Healer"], invest=0.5, bond_top=[60, 44, 36, 28], style="role"),
    dict(nick="데모 탱커 장인", level=63, owned=72, trained=21, love=["호시노", "아카네"], roles=["Tanker"], invest=0.55, bond_top=[58, 46, 36, 28], style="role"),
    dict(nick="데모 서포터 덕후", level=67, owned=94, trained=27, love=["노아", "세리카", "이오리"], roles=["Supporter"], invest=0.5, bond_top=[62, 48, 38, 30], style="role"),
    dict(nick="데모 스트라이커 파", level=71, owned=98, trained=34, love=["카즈사", "시로코", "아루"], roles=["DamageDealer"], invest=0.65, bond_top=[54, 46, 38, 30], style="role"),
    dict(nick="데모 수영복 수집", level=73, owned=116, trained=28, love=["호시노", "시로코", "미사키"], outfits=4.0, invest=0.45, bond_top=[66, 50, 40, 30], style="outfit"),
    dict(nick="데모 의상 컬렉터", level=79, owned=138, trained=30, love=["아루", "카요코", "하나코"], outfits=5.0, invest=0.4, bond_top=[60, 46, 36, 28], style="outfit"),
    dict(nick="데모 트리니티 소풍", level=61, owned=78, trained=22, love=["미카", "세리나", "하나코", "마리"], schools=["Trinity"], invest=0.5, bond_top=[62, 50, 40, 32], style="school"),
    dict(nick="데모 밀레니엄 소속", level=65, owned=90, trained=26, love=["유우카", "리오", "아리스"], schools=["Millennium"], invest=0.55, bond_top=[60, 52, 44, 34], style="school"),
    dict(nick="데모 백귀야행 순례", level=57, owned=72, trained=18, love=["이즈나", "미치루", "후부키"], schools=["Hyakkiyako"], invest=0.45, bond_top=[56, 46, 36, 28], style="school"),
    dict(nick="데모 붉은겨울 동지", level=59, owned=66, trained=17, love=["치나츠", "네루"], schools=["RedWinter"], invest=0.45, bond_top=[54, 44, 34, 26], style="school"),
    # long-time players
    dict(nick="데모 올드비 선생", level=84, owned=176, trained=70, love=["호시노", "미카", "히나"], invest=0.85, bond_top=[100, 86, 74, 64, 52, 44], style="veteran"),
    dict(nick="데모 풀돌파 고인물", level=85, owned=214, trained=96, love=["아루", "카요코", "유우카", "아리스"], invest=0.95, bond_top=[100, 92, 78, 66, 54, 46], style="veteran"),
    dict(nick="데모 무과금 장인", level=80, owned=120, trained=48, love=["시로코", "노노미"], invest=0.75, bond_top=[82, 60, 48, 40], style="veteran"),
    dict(nick="데모 인연 전도사", level=75, owned=102, trained=26, love=["호시노", "시로코", "유우카", "노아", "미카", "히나", "카요코", "아리스"], invest=0.5, bond_top=[100, 88, 76, 68, 60, 54, 48, 42], style="bond"),
    dict(nick="데모 한 우물", level=77, owned=100, trained=20, love=["아즈사"], invest=0.55, bond_top=[100, 36, 28, 20], style="bond"),
    # heavy users: nearly everyone, deep investment, still not every limited student
    dict(nick="데모 헤비 유저 A", level=88, owned=252, trained=150, love=["호시노", "미카", "히나", "아루", "카요코"], invest=0.95, spend=0.99, luck=0.55, bond_top=[100, 94, 82, 70, 60, 52, 46], style="heavy"),
    dict(nick="데모 헤비 유저 B", level=90, owned=262, trained=176, love=["유우카", "노아", "리오", "아리스", "케이"], invest=0.97, spend=0.99, luck=0.7, bond_top=[100, 96, 84, 72, 62, 54], style="heavy"),
    dict(nick="데모 풀소유 장인", level=86, owned=240, trained=120, love=["시로코", "세리카", "노노미"], invest=0.9, spend=0.98, luck=0.4, bond_top=[100, 88, 76, 64, 52], style="heavy"),
    dict(nick="데모 과금 회장", level=87, owned=246, trained=132, love=["카즈사", "나츠", "미카", "하나코"], invest=0.92, spend=0.98, luck=0.8, bond_top=[100, 90, 78, 66, 54, 44], style="heavy"),
    dict(nick="데모 헤비 유저 C", level=84, owned=228, trained=96, love=["이즈나", "와카모", "치나츠"], invest=0.85, spend=0.95, luck=0.35, bond_top=[88, 72, 60, 50, 42], style="heavy"),
    dict(nick="데모 4년차 올드비", level=89, owned=234, trained=140, love=["호시노", "시로코", "유우카"], invest=0.93, spend=0.9, luck=0.5, bond_top=[100, 84, 72, 60, 50], style="heavy"),
    # luck
    dict(nick="데모 뽑기 불운", level=72, owned=84, trained=22, love=["미카", "히나", "호시노", "카요코", "아루"], invest=0.55, spend=0.1, luck=0.05, bond_top=[60, 44, 34], style="unlucky"),
    dict(nick="데모 천장 인생", level=68, owned=76, trained=18, love=["아리스", "케이", "유우카", "리오"], invest=0.5, spend=0.18, luck=0.1, bond_top=[54, 40, 30], style="unlucky"),
    dict(nick="데모 광탈 또 광탈", level=61, owned=64, trained=14, love=["카즈사", "호시노", "미카"], invest=0.4, spend=0.1, luck=0.0, bond_top=[44, 32], style="unlucky"),
    dict(nick="데모 첫 뽑기 대박", level=44, owned=58, trained=14, love=["미카", "호시노", "히나", "아리스"], invest=0.4, spend=0.98, luck=0.95, bond_top=[40, 32, 24, 18], style="lucky"),
    dict(nick="데모 무과금 행운", level=55, owned=79, trained=18, love=["카요코", "유우카", "아루"], invest=0.45, spend=0.9, luck=0.9, bond_top=[50, 38, 28], style="lucky"),
    dict(nick="데모 중복의 신", level=70, owned=96, trained=26, love=["시로코"], invest=0.5, spend=0.5, luck=1.0, bond_top=[70, 40], style="lucky"),
]

NICKS = ["데모 방랑 선생", "데모 샬레 야근러", "데모 커피 한 잔", "데모 새벽 출석", "데모 말랑 선생", "데모 소원 수리", "데모 용돈 털이", "데모 인형뽑기",
         "데모 일일 숙제", "데모 레이드 전사", "데모 총력전 낙오", "데모 모모톡 읽씹", "데모 간식 배달", "데모 순찰 중", "데모 반려 학생", "데모 안전 제일",
         "데모 휴가 중", "데모 천천히 가자", "데모 이벤트 놓침", "데모 월급 루팡", "데모 복귀 선생", "데모 쿨타임 대기", "데모 오늘도 출근", "데모 느긋한 하루"]


def random_personas(known, count=24, seed=20261124):
    """More teachers of every size and taste from a fixed seed: levels, roster sizes and loved students all differ."""
    import random
    rng = random.Random(seed)
    loved_pool = sorted(b for b in set(TOP10) | set(WIDELY_LOVED) if b in known)
    out = []
    for i in range(count):
        level = rng.choice([rng.randint(15, 40), rng.randint(35, 65), rng.randint(55, 85), rng.randint(60, 80)])
        owned = max(14, min(200, int((12 + (level - 10) * 1.6) * rng.uniform(0.65, 1.5))))
        love = rng.sample(loved_pool, rng.choice([0, 1, 1, 2, 3, 4]))
        trained = max(3, int(owned * rng.uniform(0.12, 0.5)))
        tops = sorted((int(min(level, 99) * rng.uniform(0.2, 1.0)) for _ in range(rng.randint(2, 5))), reverse=True)
        out.append(dict(nick=NICKS[i % len(NICKS)], level=level, owned=owned, trained=trained, love=love,
                        invest=round(rng.uniform(0.25, 0.85), 2), bond_top=tops or [8], outfits=rng.choice([1.0, 1.0, 1.5, 2.5]), style="random",
                        schools=rng.choice([[], [], ["Gehenna"], ["Trinity"], ["Millennium"], ["Abydos"], ["Hyakkiyako"]])))
    return out
