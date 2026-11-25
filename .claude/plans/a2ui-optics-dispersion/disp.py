import math
def height(x):
    x=min(max(x,0),1); return max(1-(1-x)**4,0)**0.25
def slope(x):
    dx=1e-3; return (height(min(x+dx,1))-height(max(x-dx,0)))/(2*dx)
BEZ,TH,DEP=17.0,16.0,8.0
def disp(x,ior):
    h=height(x); d=slope(x)*(TH/BEZ); nl=math.sqrt(d*d+1); nx,nz=d/nl,1/nl
    eta=1/ior; k=1-eta*eta*(1-nz*nz)
    if k<0: return 0.0
    m=eta*nz-math.sqrt(k); tx=m*nx; tz=-eta+m*nz
    return tx/abs(tz)*(h*TH+DEP)
N=128
def cs(ior): return [disp(i/N,ior) for i in range(N+1)]
LC,LD,LF=656.3,587.6,486.1
def cauchy(nd,V):
    dn=(nd-1)/V; B=dn/(1/LF**2-1/LC**2); A=nd-B/LD**2
    return lambda l: A+B/l**2
glasses={'BK7 (nd 1.5168 V 64.17)':(1.5168,64.17),'engine ior1.5 V64':(1.5,64.0),'F2 (nd 1.6200 V 36.37)':(1.62004,36.37),'SF10 (nd 1.7283 V 28.41)':(1.72825,28.41)}
for name,(nd,V) in glasses.items():
  for lab,(lr,lg,lb) in {'C/d/F':(LC,LD,LF),'sRGB dom 612/549/465':(612,549,465)}.items():
    n=cauchy(nd,V); nr,ng,nb=n(lr),n(lg),n(lb)
    R,G,Bc=cs(nr),cs(ng),cs(nb)
    mx=max(abs(v) for v in G)
    sep=[abs(b-r) for r,b in zip(R,Bc)]
    i=max(range(N+1),key=lambda j:sep[j])
    print(f"{name:26s} {lab:22s} nR={nr:.5f} nG={ng:.5f} nB={nb:.5f} |G|max={mx:.3f}px  R-B max sep={sep[i]:.3f}px at x={i/N:.3f} (G={G[i]:.2f})")
# ratio constancy, engine glass
nd,V=1.5,64.0; n=cauchy(nd,V)
R,G,Bc=cs(n(LC)),cs(n(LD)),cs(n(LF))
print("\nx, G px, R/G, B/G, B-R px  (ior 1.5, V 64)")
for i in [0,1,2,4,6,8,12,16,24,32,48,64,96,127]:
    g=G[i]; print(f"{i/N:.3f} {g:8.3f} {R[i]/g if g else float('nan'):.5f} {Bc[i]/g if g else float('nan'):.5f} {Bc[i]-R[i]:7.3f}")
# scale-only approximation: per-channel scale = max_c/max_G applied to G shape
for (nd,V) in [(1.5,64.0),(1.62004,36.37),(1.72825,28.41)]:
  n=cauchy(nd,V);G=cs(n(LD))
  for c,l in [('R',LC),('B',LF)]:
    E=cs(n(l)); s=max(abs(v) for v in E)/max(abs(v) for v in G)
    err=[abs(E[i]-s*G[i]) for i in range(N+1)]; sepc=[abs(E[i]-G[i]) for i in range(N+1)]
    print(f"nd {nd} V {V} ch {c}: scale-only max err {max(err):.4f}px vs channel offset max {max(sepc):.4f}px ({100*max(err)/max(sepc):.1f}%) ")
# TIR check: where is k<0 near rim
print("\nrim samples G:", [round(v,2) for v in G[:6]])
