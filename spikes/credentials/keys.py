import json,sys
def walk(o,p=""):
    for k,v in o.items():
        if isinstance(v,dict):
            print(p+k+" {}"); walk(v,p+"  ")
        else: print(p+k+": "+type(v).__name__)
walk(json.load(open(sys.argv[1])))
