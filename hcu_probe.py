from pathlib import Path
from PIL import Image
import hcu_sdk as h
import win32gui, win32api, win32con

d=h.connect()
w=next(w for w in d.apps().windows if w.title=='Blue Archive')
hwnd=win32gui.FindWindow(None,'Blue Archive')
left,top=win32gui.ClientToScreen(hwnd,(0,0)); _,_,width,height=win32gui.GetClientRect(hwnd)
frame=d.screenshot(); fw,fh=frame.screen.width,frame.screen.height
vx=win32api.GetSystemMetrics(win32con.SM_XVIRTUALSCREEN); vy=win32api.GetSystemMetrics(win32con.SM_YVIRTUALSCREEN)
vw=win32api.GetSystemMetrics(win32con.SM_CXVIRTUALSCREEN); vh=win32api.GetSystemMetrics(win32con.SM_CYVIRTUALSCREEN)
x0=(left-vx)*fw/vw;x1=(left+width-vx)*fw/vw;y0=(top-vy)*fh/vh;y1=(top+height-vy)*fh/vh
print('frame',fw,fh,'client',x0,y0,x1,y1)
parts=[]
for ya,yb in ((y0,(y0+y1)/2),((y0+y1)/2,y1)):
 z=d.zoom([int(x0)-2,int(ya)-2,int(x1)+2,int(yb)+2]); region=z.evidence.zoom.region; sx=z.evidence.zoom.scale_x; sy=z.evidence.zoom.scale_y
 img=Image.open(z.images[0]); box=((x0-region[0])*sx,(ya-region[1])*sy,(x1-region[0])*sx,(yb-region[1])*sy)
 part=img.crop(tuple(round(v) for v in box)).resize((width,height//2))
 print('zoom',img.size,'box',box,'part',part.size)
 parts.append(part)
out=Image.new('RGB',(width,height)); out.paste(parts[0],(0,0));out.paste(parts[1],(0,height//2))
out.save('D:/work-github/bluearchive-ocr/output-exe/hcu-capture.png')
