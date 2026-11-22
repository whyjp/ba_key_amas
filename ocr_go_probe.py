from PIL import Image,ImageOps,ImageEnhance
import subprocess,io
im=Image.open('output-go-live/detail.png')
e='vendor/ocr/Library/bin/tesseract.exe'
rects={'basic1':(790,516,905,551),'basic2':(800,520,883,548),'basic3':(805,522,880,546),'ex':(694,522,767,547),'passive':(908,522,987,547),'weapon':(778,570,827,600),'weapon2':(766,563,850,608),'weapon3':(775,575,829,602)}
for k,b in rects.items():
 for mode in ('color','gray','contrast','bw'):
  c=im.crop(b).resize(((b[2]-b[0])*6,(b[3]-b[1])*6))
  if mode!='color':c=ImageOps.grayscale(c)
  if mode=='contrast':c=ImageEnhance.Contrast(c).enhance(2)
  if mode=='bw':c=c.point(lambda v: 0 if v<160 else 255)
  buf=io.BytesIO();c.save(buf,'PNG')
  for psm in (6,7,11,13):
   r=subprocess.run([e,'stdin','stdout','-l','eng','--psm',str(psm),'--tessdata-dir','vendor/ocr/tessdata'],input=buf.getvalue(),capture_output=True)
   print(k,mode,psm,repr(r.stdout.decode().strip()))
