exec(open('disp.py').read().split('glasses=')[0])
def sep(V,nd=1.5):
    n=cauchy(nd,V); R,B=cs(n(LC)),cs(n(LF)); return max(abs(b-r) for r,b in zip(R,B))
for V in [64,32,16,10,8,5]: print(V, round(sep(V),3))
print("ratio V32/V64", sep(32)/sep(64))
print("map LSB px at scale 2*14.097:", 2*14.097/255)
