#!/usr/bin/env python3
"""Additional synthetic cases, designed after implementation and not tuned against.
Expected shared-fragment acceptance is independently computed by Python re.
"""
import json, random, re
from pathlib import Path
rng=random.Random(739102)
work=[]
def letters(n): return ''.join(rng.choice('abcdefghijklmnopqrstuvwxyz') for _ in range(n))
def add(id,pattern,make,regular=True,baseline=None,post='none'):
    baseline=baseline or pattern
    py=re.sub(r'\\k<([A-Za-z_][A-Za-z_0-9]*)>',r'(?P=\1)',baseline)
    py=re.sub(r'\(\?<([A-Za-z_][A-Za-z_0-9]*)>',r'(?P<\1>',py)
    rx=re.compile(py,re.ASCII)
    inputs=[]
    for i in range(256):
        s=make(i);m=rx.fullmatch(s)
        ok=m is not None
        if ok and post=='len16':ok=len(m['x'])<=16
        if ok and post=='lower_admin':ok=m['x'].lower()=='admin'
        if ok and post=='upper_pair':ok=m['x'].upper()==m['y']
        inputs.append(dict(text=s,expected=ok))
    work.append(dict(id=id,pattern=pattern,baseline=baseline,regular=regular,post=post,params={},inputs=inputs))
def mutate(s,i):
    if i%4==0:return s
    if i%4==1:return '#'+s[1:]
    if i%4==2:return s[:len(s)//2]+'#'+s[len(s)//2+1:]
    return s+'#'
for n in [16,128,512]:
    lit=letters(n)
    add(f'literal_{n}',lit,lambda i,s=lit:mutate(s,i))
add('overlapping_alternatives',r'(?:foo|foobar|foobaz):[0-9]+',lambda i:mutate(['foo','foobar','foobaz'][i%3]+':'+str(100+i),i))
add('disjoint_alternatives',r'(?:get|post|delete|patch):[a-z]{1,40}',lambda i:mutate(['get','post','delete','patch'][(i//4)%4]+':'+letters(1+i%40),i))
add('eight_captures',':'.join(r'(?<c%d>[a-z]{1,12})'%j for j in range(8)),lambda i:mutate(':'.join(letters(1+(i+j)%12) for j in range(8)),i))
def repeated(i):
    s=''.join(rng.choice('ab') for _ in range(1+i%16));return s+s if i%2==0 else s+s+'c'
add('backref_no_delimiter',r'(?<x>[ab]{1,16})\k<x>',repeated,False)
def longref(i):
    s=letters(64+i%448);return mutate(s+':'+s,i)
add('backref_long_mixed_failures',r'(?<x>[a-z]{64,512}):\k<x>',longref,False)
add('lazy_delimiter',r'(?<key>[a-z]+?)=(?<value>[0-9]+)',lambda i:mutate(letters(1+i%80)+'='+str(100+i),i))
def repeated_capture(i):
    seq=[''.join(rng.choice('ab') for _ in range(1+(i+j)%3)) for j in range(1+i%5)]
    return mutate('-'.join(seq)+'-'+seq[-1],i)
add('repeated_capture',r'(?:(?<x>[ab]{1,3})-){1,5}\k<x>',repeated_capture,False)
add('guard_long',r'(?<x>[A-Za-z]+)(?{(call (lambda (s Str) (eq (lower s) "admin")) (cap x))})',lambda i:'aDmIn' if i%4==0 else letters(16+i%200),baseline=r'(?<x>[A-Za-z]+)',post='lower_admin')
def emit(i):
    s=letters(1+i%64);return mutate(s+':'+s.upper(),i)
add('emit_lengths',r'(?<x>[a-z]{1,64}):${(upper (cap x))}',emit,baseline=r'(?<x>[a-z]{1,64}):(?<y>[A-Z]{1,64})',post='upper_pair')
add('wide_class',r'[!-~]{4,256}',lambda i:''.join(chr(rng.randrange(33,127)) for _ in range(4+i%250))+(' ' if i%2 else ''))
add('negated_class',r'(?<x>[^;\n]*);[0-9]+',lambda i:letters(i%128)+';'+str(i)+('\n' if i%2 else ''))
add('class_scan_4096',r'(?<x>[a-z]+)',lambda i:mutate(letters(1024+i%3072),i))
add('length_guard_scale',r'(?<x>[a-z]+)(?{(le (len (cap x)) 16)})',lambda i:letters(1+i%16 if i%2==0 else 64+i%448),baseline=r'(?<x>[a-z]+)',post='len16')
add('branch_captures',r'(?:(?<a>[a-z]{1,8}):[0-9]+|(?<b>[0-9]{1,8}):[a-z]+)',lambda i:mutate(letters(1+i%8)+':'+str(i) if i%2 else str(i)+':'+letters(1+i%24),i))
p=Path(__file__).resolve().parents[1]/'benchmarks/validation-corpus.json'
p.write_text(json.dumps(work,indent=2)+'\n')
print(len(work),'additional workloads',sum(len(w['inputs']) for w in work),'inputs')
