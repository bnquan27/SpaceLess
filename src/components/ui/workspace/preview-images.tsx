const serverTiles = new Map([
  ["uiux", 0],
  ["dev", 1],
  ["sec", 2],
  ["game", 3],
]);

const portraitTiles = new Map(
  [
    "Nguyễn Minh", "Hoàng Đạt", "Thu Hà", "Quốc Bảo",
    "Minh Anh", "Hoàng Long", "Đăng Khoa", "Tuấn Nghĩa",
    "Khánh Linh", "Việt Hoàng", "Phương Thảo", "Đức Mạnh",
    "Trần Văn Tuấn Đình", "Phạm Tuấn Anh", "Bùi Nam Quân",
  ].map((name, index) => [name, index]),
);

function PreviewPhoto({ sheet, index }: { sheet: "servers" | "avatars"; index: number }) {
  const columns = sheet === "servers" ? 2 : 4;
  const x = ((index % columns) / (columns - 1)) * 100;
  const y = (Math.floor(index / columns) / (columns - 1)) * 100;

  return (
    <span
      aria-hidden="true"
      data-preview-photo={sheet}
      className="pointer-events-none absolute inset-0 rounded-[inherit] bg-no-repeat"
      style={{
        backgroundImage: `url(/assets/workspace-${sheet}.jpg)`,
        backgroundPosition: `${x}% ${y}%`,
        backgroundSize: `${columns * 100}% ${columns * 100}%`,
      }}
    />
  );
}

export function PreviewServerPhoto({ id }: { id: string }) {
  const backend = useBackend();
  if (backend) return <BackendPhoto path={backend.data.channels.find(channel => channel.channel_id === id)?.icon_url}/>;
  const tile = serverTiles.get(id);
  return tile === undefined ? null : <PreviewPhoto sheet="servers" index={tile} />;
}

export function PreviewAvatar({ name, initials }: { name: string; initials: string }) {
  const backend = useBackend();
  if (backend) {
    const clean = name.replace(/ \(Bạn\)$/,'');
    const person = [backend.data.profile,...backend.data.friends,...backend.data.channels.flatMap(channel => channel.members)].find(person => person.full_name === clean);
    return <span aria-hidden="true" className="absolute inset-0 grid place-items-center rounded-[inherit]">{initials}<BackendPhoto path={person?.avatar_url}/></span>;
  }
  const tile = portraitTiles.get(name.replace(/ \(Bạn\)$/, ""));
  return (
    <span aria-hidden="true" className="absolute inset-0 grid place-items-center rounded-[inherit]">
      {initials}
      {tile !== undefined && <PreviewPhoto sheet="avatars" index={tile} />}
    </span>
  );
}
import { useEffect, useState } from 'react';
import { useBackend } from '@/lib/workspace-backend';


function BackendPhoto({path}: {path?:string|null}) {
  const backend = useBackend();
  const [src,setSrc] = useState('');
  useEffect(() => {
    let active = true; let objectUrl=''; setSrc('');
    if (backend && path) void backend.api.request<{url:string}>(path.replace(/^\/api\/v1/,'')).then(value => backend.api.download(value.url)).then(blob => { if (active) { objectUrl=URL.createObjectURL(blob); setSrc(objectUrl); } }).catch(() => {});
    return () => {active=false;if(objectUrl)URL.revokeObjectURL(objectUrl);};
  },[backend?.api,path]);
  return src ? <img src={src} alt="" className="pointer-events-none absolute inset-0 h-full w-full rounded-[inherit] object-cover"/> : null;
}
