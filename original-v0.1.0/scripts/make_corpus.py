#!/usr/bin/env python3
"""Deterministic synthetic byte-oriented corpus; no sampled external data."""
import json
from pathlib import Path

def letters(i,n):
    return ''.join(chr(97+((i*17+j*7+j*j)%26)) for j in range(n))
def cases(make):
    return [dict(zip(('text','expected'),make(i))) for i in range(256)]
work=[]
def add(id,pattern,baseline,make,regular=True,post='none',params=None):
    work.append(dict(id=id,pattern=pattern,baseline=baseline,regular=regular,post=post,params=params or {},inputs=cases(make)))
literal='protocol:tenant:west:service:mail:revision:2026:complete'
add('literal',literal,literal,lambda i:(literal if i%2==0 else literal[:-1]+'x',i%2==0))
add('identifier',r'(?<id>[a-z][a-z0-9_]{0,31})',r'(?<id>[a-z][a-z0-9_]{0,31})',lambda i:(letters(i,1+i%31)+('' if i%2==0 else '#'),i%2==0))
add('fields',r'(?<key>[a-z]+)=(?<value>[0-9]+)',r'(?<key>[a-z]+)=(?<value>[0-9]+)',lambda i:(letters(i,4+i%20)+'='+str(10000+i)+('' if i%2==0 else 'x'),i%2==0))
add('alternation',r'(?:alpha|beta|gamma|delta):[0-9]{4}',r'(?:alpha|beta|gamma|delta):[0-9]{4}',lambda i:((['alpha','beta','gamma','delta'][i%4])+':'+str(1000+i)+('' if i%2==0 else 'x'),i%2==0))
def back(i,lo,span):
    s=letters(i,lo+i%span);return s+'-'+(s if i%2==0 else s[:-1]+'#'),i%2==0
add('backref_short',r'(?<x>[a-z]{4,12})-\k<x>',r'(?<x>[a-z]{4,12})-\k<x>',lambda i:back(i,4,9),regular=False)
add('backref_long',r'(?<x>[a-z]{64,256})-\k<x>',r'(?<x>[a-z]{64,256})-\k<x>',lambda i:back(i,64,193),regular=False)
prefix='tenant:research:queue:'
add('static_parameter',r'${(param prefix)}(?<id>[0-9]{4})(?{(eq (upper "ok") "OK")})',prefix+r'(?<id>[0-9]{4})',lambda i:(prefix+str(1000+i)+('' if i%2==0 else 'x'),i%2==0),params={'prefix':prefix})
add('lambda_guard',r'(?<x>[A-Za-z]+)(?{(call (lambda (s Str) (eq (lower s) (lower "ADMIN"))) (cap x))})',r'(?<x>[A-Za-z]+)',lambda i:((['admin','AdMiN','ADMIN','Admin'][i%4] if i%2==0 else letters(i,5+i%30)),i%2==0),post='lower_admin')
def upper(i):
    x=letters(i,4+i%13);y=x.upper();return x+':'+(y if i%2==0 else ('Z' if y[0]!='Z' else 'Y')+y[1:]),i%2==0
add('dynamic_interpolation',r'(?<x>[a-z]{4,16}):${(upper (cap x))}',r'(?<x>[a-z]{4,16}):(?<y>[A-Z]{4,16})',upper,post='upper_pair')
add('length_guard',r'(?<x>[a-z]+)(?{(le (len (cap x)) 16)})',r'(?<x>[a-z]+)',lambda i:(letters(i,1+i%32),1+i%32<=16),post='len16')
add('constant_lambda',r'(?{(call (lambda (s Str) (and (eq (upper s) "TOKEN") (eq (len s) 5))) (lower "TOKEN"))})[a-z]+',r'[a-z]+',lambda i:(letters(i,4+i%28)+('' if i%2==0 else '#'),i%2==0))
path=Path(__file__).resolve().parents[1]/'benchmarks/corpus.json'
path.write_text(json.dumps(work,indent=2)+'\n')
print(f'{len(work)} workloads, {sum(len(w["inputs"]) for w in work)} inputs')
