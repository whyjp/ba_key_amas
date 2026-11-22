"""OCR engine comparison on labelled fixture crops (dev tool, not product code).

Tesseract numbers came from the Go pipeline of commit d3c5178 (cmd/baocr, test BenchDump; the Go code
has since been removed). Kept as the record of the engine comparison in docs/ARCHITECTURE.md.
Usage:  python bench.py tess.json [rec|detrec|recpad]
Needs: pip install rapidocr onnxruntime. Ground truth was read off the fixtures by eye.
"""
import json,re,sys,time
import numpy as np
from PIL import Image
from rapidocr import RapidOCR, LangRec, OCRVersion, ModelType
F='D:/work-github/bluearchive-ocr/fixtures/'
tess=json.load(open(sys.argv[1],encoding='utf-8'))
eng=RapidOCR(params={"Rec.lang_type":LangRec.KOREAN,"Rec.ocr_version":OCRVersion.PPOCRV5,"Rec.model_type":ModelType.MOBILE})
MODE=sys.argv[2] if len(sys.argv)>2 else 'rec'
def rec(img,box,scale=4):
    base=Image.open(F+img).convert('RGB')
    x0,y0,x1,y1=box
    im=base.crop((x0-12,y0-8,x1+12,y1+8)) if MODE!='rec' else base.crop(box)
    im=im.resize((im.width*(scale if MODE=='rec' else 2),im.height*(scale if MODE=='rec' else 2)),Image.BICUBIC if MODE!='rec' else Image.LANCZOS)
    if MODE in('rec','recpad'):
        r=eng(np.array(im),use_det=False,use_cls=False,use_rec=True)
    else:
        r=eng(np.array(im),use_det=True,use_cls=False,use_rec=True)
    t=''.join(r.txts) if r.txts else ''
    return t
def digits(t): 
    m=re.findall(r'\d+',t.replace(',',''))
    return int(m[-1]) if m else None
def first_int(t):
    m=re.findall(r'\d+',t)
    return int(m[0]) if m else None
names={'detail-shun.png':'슌','detail-rio-armed.png':'리오(무장)','detail-saori.png':'사오리(드레스)','detail-satsuki-swimsuit.png':'사츠키(수영복)','detail-shiroko-terror.png':'시로코*테러','detail-rio-special.png':'리오','detail-hoshino-armed.png':'호시노(무장)','detail-hoshino-plain.png':'호시노','detail-serika-newyear-lv1.png':'세리카(새해)','detail-tsurugi-swimsuit-lv1-locked.png':'츠루기(수영복)','detail-rumi-terrain1.png':'루미'}
stats={'detail-shun.png':[20221,4343,86,4808],'detail-rio-armed.png':[33907,3444,85,4848],'detail-serika-newyear-lv1.png':[2290,260,18,2064],'detail-tsurugi-swimsuit-lv1-locked.png':[2536,354,136,1379]}
skills={'detail-shun.png':[2,3,3,2],'detail-serika-newyear-lv1.png':[1,1,1,1],'detail-saori.png':[2,4,5,3]}
exp={'detail-shun.png':(3,21790),'detail-serika-newyear-lv1.png':(0,10),'detail-saori.png':(15,21790),'detail-rio-armed.png':(41,21790)}
level={'detail-shun.png':68,'detail-saori.png':68,'detail-serika-newyear-lv1.png':1,'detail-tsurugi-swimsuit-lv1-locked.png':1,'detail-rio-armed.png':68}
bond={'detail-shun.png':11,'detail-rio-armed.png':13,'detail-saori.png':5,'detail-serika-newyear-lv1.png':2,'detail-shiroko-terror.png':12,'detail-satsuki-swimsuit.png':4,'detail-rio-special.png':2,'detail-tsurugi-swimsuit-lv1-locked.png':4}
def norm(n): return re.sub(r'[^가-힣()*]','',n.replace('^','*').replace('＊','*'))
res={}
def score(field,items):
    ok_t=ok_p=0;rows=[]
    for k,(gt,t,p) in items.items():
        ok_t+= (t==gt); ok_p+= (p==gt); rows.append((k.replace('detail-','').replace('.png',''),gt,t,p))
    res[field]=(ok_t,ok_p,len(items),rows)
# names
items={}
for f,gt in names.items():
    p=norm(rec(f,(64,789,300,835)))
    items[f]=(gt,tess[f]['name'],p)
score('name',items)
# stats (4 per image)
items={}
boxes=[(684,350,860,374),(890,350,1060,374),(684,386,860,410),(890,386,1060,410)]
for f,gt in stats.items():
    for i,b in enumerate(boxes):
        t=[tess[f]['stats'][k] for k in ['max_hp','attack','defense','healing']][i]
        items[f+str(i)]=(gt[i],t,digits(rec(f,b)))
score('stats',items)
# skills
sb=[(694,522,767,547),(800,520,883,548),(908,522,987,547),(1020,520,1090,548)]
items={}
for f,gt in skills.items():
    for i,b in enumerate(sb):
        items[f+str(i)]=(gt[i],tess[f]['skills'][i],digits(rec(f,b)))
score('skill levels',items)
items={}
for f,(c,r) in exp.items():
    t=tess[f]['exp']; p=rec(f,(210,833,390,858)); m=re.findall(r'\d+',p.replace(',',''))
    items[f]=((c,r),(t['current'],t['required']),(int(m[0]),int(m[1])) if len(m)>=2 else None)
score('exp',items)
items={f:(g,tess[f]['level'],digits(rec(f,(30,827,104,865)))) for f,g in level.items()}
score('level (strip)',items)
items={f:(g,tess[f]['bond'],digits(rec(f,(34,794,84,832)))) for f,g in bond.items()}
score('bond (heart)',items)
# popup
pg={'main':[33907,3444,85,4848],'acc':101,'eva':1079,'crit':523,'critdmg':234,'pierce':0,'stab':980,'range':550,'cost':700,'ccp':100,'ccr':124,'bonus':200,'maxcost':0}
pp=tess['popup']; tv={'acc':pp['accuracy'],'eva':pp['evasion'],'crit':pp['critical'],'critdmg':pp['critical_damage_pct'],'pierce':pp['defense_pierce'],'stab':pp['stability'],'range':pp['range'],'cost':pp['cost_recovery'],'ccp':pp['cc_power'],'ccr':pp['cc_resist'],'bonus':pp['type_bonus_pct'],'maxcost':pp['max_cost_bonus']}
cells={'acc':('popup-stats-top.png',(232,462,640,504)),'eva':('popup-stats-top.png',(650,462,1062,504)),'crit':('popup-stats-top.png',(232,532,640,574)),'critdmg':('popup-stats-top.png',(650,532,1062,574)),'pierce':('popup-stats-top.png',(232,599,640,641)),'stab':('popup-stats-top.png',(650,599,1062,641)),
'range':('popup-stats-bottom.png',(232,365,640,407)),'cost':('popup-stats-bottom.png',(650,365,1062,407)),'ccp':('popup-stats-bottom.png',(232,432,640,474)),'ccr':('popup-stats-bottom.png',(650,432,1062,474)),'bonus':('popup-stats-bottom.png',(232,499,640,541)),'maxcost':('popup-stats-bottom.png',(650,499,1062,541))}
items={k:(pg[k],tv[k],digits(rec(f,b,3))) for k,(f,b) in cells.items()}
score('popup rows',items)
t0=time.time(); n=0
for f in list(names)[:5]:
    rec(f,(64,789,300,835)); n+=1
dt=(time.time()-t0)/n
tot_t=sum(v[0] for v in res.values()); tot_p=sum(v[1] for v in res.values()); tot=sum(v[2] for v in res.values())
print(f"{'field':16}{'Tesseract(tuned)':>18}{'PP-OCRv5 ko(plain)':>20}{'n':>5}")
for k,(a,b,n_,rows) in res.items(): print(f"{k:16}{a:>14}/{n_:<3}{b:>16}/{n_:<3}")
print(f"{'TOTAL':16}{tot_t:>14}/{tot:<3}{tot_p:>16}/{tot:<3}   rapid ~{dt*1000:.0f} ms/crop")
print('--- PP-OCR misses / Tesseract misses')
for k,(a,b,n_,rows) in res.items():
    for r in rows:
        if r[3]!=r[1] or r[2]!=r[1]: print(f"{k}: {r[0]} gt={r[1]} tess={r[2]} paddle={r[3]}")
