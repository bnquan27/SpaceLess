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
  const tile = serverTiles.get(id);
  return tile === undefined ? null : <PreviewPhoto sheet="servers" index={tile} />;
}

export function PreviewAvatar({ name, initials }: { name: string; initials: string }) {
  const tile = portraitTiles.get(name.replace(/ \(Bạn\)$/, ""));
  return (
    <span aria-hidden="true" className="absolute inset-0 grid place-items-center rounded-[inherit]">
      {initials}
      {tile !== undefined && <PreviewPhoto sheet="avatars" index={tile} />}
    </span>
  );
}
