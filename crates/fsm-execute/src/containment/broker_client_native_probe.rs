//! Unchanged independent UID and frame producer for the provisioned broker.

pub(super) const CLIENT: &str = r#"import errno,json,os,socket,sys
uid=int(sys.argv[1])
os.setgroups([])
os.setgid(uid)
os.setuid(uid)
assert os.getuid()==uid and os.geteuid()==uid and os.getgroups()==[]
route=json.load(open(sys.argv[2]+'/route.json'))
path=sys.argv[2]+'/s-'+str(route['epoch'])
s=socket.socket(socket.AF_UNIX)
s.settimeout(5)
if sys.argv[3]=='deny':
    try:
        s.connect(path)
    except OSError as error:
        assert error.errno==errno.EACCES
        sys.exit(0)
    raise AssertionError('unauthorized client connected')
from pathlib import Path
base=Path(sys.argv[2])
encoded=sys.argv[3].encode()
reader,writer=os.pipe()
framed=len(encoded).to_bytes(4,'big')+encoded
while framed:
    count=os.write(writer,framed)
    framed=framed[count:]
os.close(writer)
os.dup2(reader,0)
os.close(reader)
authority=base.parent
namespace=authority.parent.name
generation=authority.name.removeprefix('authority-')
os.execv('/usr/libexec/fsm-containment-authority',['fsm-containment-authority','client',namespace,generation])
"#;
