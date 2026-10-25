#!/usr/bin/env python3
"""Fresh synthetic validation; labels from Python re, before seeing its timings."""
import json,re,random
from pathlib import Path
rng=random.Random(29101987);work=[]
def letters(n,alphabet='abcdefghijklmnopqrstuvwxyz'):return ''.join(rng.choice(alphabet) for _ in range(n))
def mutate(s,i):
    if i%4==0:return s
    if i%4==1:return '#'+s[1:]
    if i%4==2:return s[:-1]+'#'
    return s+'#'
def add(id,pattern,make,baseline=None,post='none',params=None):
    baseline=baseline or pattern
    py=re.sub(r'\\k<([A-Za-z_][A-Za-z_0-9]*)>',r'(?P=\1)',baseline)
    py=re.sub(r'\(\?<([A-Za-z_][A-Za-z_0-9]*)>',r'(?P<\1>',py)
    rx=re.compile(py,re.ASCII);inputs=[]
    for i in range(256):
        s=make(i);m=rx.fullmatch(s);ok=m is not None
        if ok and post=='len16':ok=len(m['x'])<=16
        if ok and post=='lower_admin':ok=m['x'].lower()=='admin'
        if ok and post=='upper_pair':ok=m['x'].upper()==m['y']
        if ok and post=='len16_not_b':ok=len(m['x'])<=16 and m['x']!='b'
        inputs.append(dict(text=s,expected=ok))
    work.append(dict(id='r3_'+id,pattern=pattern,baseline=baseline,regular=False,post=post,params=params or {},inputs=inputs))
def copies(i,k,pre='',middle='',suffix=''):
    s=letters([0,1,2,7,16,64,128,256][(i//4)%8],'ab')
    return mutate(pre+s+middle+s*(k-1)+suffix,i)
add('triple_backref',r'(?<x>[ab]{0,256})\k<x>\k<x>',lambda i:copies(i,3))
add('affine_prefix_suffix',r'header:(?<x>[ab]{0,256})a\k<x>:footer',lambda i:copies(i,2,'header:','a',':footer'))
def nested(i):
    s=letters([0,1,7,16,64,128][(i//4)%6],'ab');v='P'+s+'Q';return mutate(v+v,i)
add('nested_capture',r'(?<outer>P(?<x>[ab]{0,128})Q)\k<outer>',nested)
def two(i):
    a=letters((i//4)%17,'ab');b=letters((i//8)%17,'ab');return mutate(a+b+'-'+a+b,i)
add('two_unknown_captures',r'(?<x>[ab]{0,16})(?<y>[ab]{0,16})-\k<x>\k<y>',two)
add('fixed_capture',r'(?<fixed>XY)(?<x>[ab]{1,256})\k<fixed>\k<x>',lambda i:copies(i,2,'XY','XY'))
add('strict_length_lambda',r'(?<x>[a-z]{0,128})(?{(call (lambda (v Str) (le (len v) 16)) (cap x))})',lambda i:letters([0,1,2,7,16,17,64,128][i%8]),baseline=r'(?<x>[a-z]{0,128})',post='len16')
add('folded_guard',r'(?<x>[A-Za-z]{1,128})(?{(eq (lower (cap x)) "admin")})',lambda i:'aDmIn' if i%4==0 else letters(1+i%128),baseline=r'(?<x>[A-Za-z]{1,128})',post='lower_admin')
def emit(i):
    s=letters([0,1,7,16,32,64][(i//4)%6]);return mutate('P'+s+':'+s.upper()+'Q',i)
add('emission_with_affixes',r'P(?<x>[a-z]{0,64}):${(upper (cap x))}Q',emit,baseline=r'P(?<x>[a-z]{0,64}):(?<y>[A-Z]{0,64})Q',post='upper_pair')
add('prefix_tree',r'(?<x>net|network|networking|new|news|newspaper):[0-9]{1,6}',lambda i:mutate(['net','network','networking','new','news','newspaper'][(i//4)%6]+':'+str(i),i))
add('capture_prefix_barriers',r'(?:(?<a>foo)a|(?<b>foo)b|(?<c>foobar)c):[0-9]+',lambda i:mutate(['fooa','foob','foobarc'][(i//4)%3]+':'+str(i),i))
def repetitions(i,empty=False):
    n=1+(i//4)%(8 if empty else 16)
    seq=[letters((i+j)%4 if empty else 1+(i+j)%8,'ab' if empty else 'abcdefghijklmnopqrstuvwxyz') for j in range(n)]
    sep='-' if empty else ':';return mutate(sep.join(seq)+sep+seq[-1],i)
add('repeated_capture_scale',r'(?:(?<x>[a-z]{1,8}):){1,16}\k<x>',lambda i:repetitions(i))
add('empty_repeated_capture',r'(?:(?<x>[ab]{0,3})-){1,8}\k<x>',lambda i:repetitions(i,True))
add('negated_class_controls',r'(?<x>[^\n;]{0,1000});[0-9]{1,8}',lambda i:letters([0,1,7,32,128,512,1000][(i//4)%7],'ab \x00\x01\t\x7f')+';'+str(i)+('\n' if i%4 else ''))
add('class_scan_large',r'(?<x>[A-Za-z0-9_]*)',lambda i:mutate(letters([0,1,31,128,512,2048,4096,8192][(i//4)%8],'abcXYZ019_'),i))
prefix=r'.*[tenant]:'
add('static_parameter_affine',r'${(param p)}(?<x>[ab]{0,256})\k<x>',lambda i:copies(i,2,prefix),baseline=re.escape(prefix)+r'(?<x>[ab]{0,256})\k<x>',params={'p':prefix})
add('generic_guard_fallback',r'(?<x>[ab]{0,32})(?{(and (le (len (cap x)) 16) (not (eq (cap x) "b")))})',lambda i:letters(i%33,'ab'),baseline=r'(?<x>[ab]{0,32})',post='len16_not_b')
dest=Path(__file__).resolve().parents[1]/'benchmarks/revision3-corpus.json'
dest.write_text(json.dumps(work,indent=2)+'\n')
print(len(work),'fresh workloads',sum(len(w['inputs']) for w in work),'inputs')
