export function AuthBrandPanel() {
  return (
    <section className="relative hidden w-[440px] shrink-0 flex-col overflow-hidden bg-gradient-to-b from-[#b4704f] via-[#9C5A42] to-[#7f4531] lg:flex">
      {/* Decorative soft circles */}
      <div className="pointer-events-none absolute -left-24 -top-24 h-72 w-72 rounded-full bg-white/10" />
      <div className="pointer-events-none absolute -bottom-28 -right-20 h-80 w-80 rounded-full bg-white/[0.08]" />
      <div className="pointer-events-none absolute right-10 top-1/3 h-24 w-24 rounded-full bg-white/[0.06]" />

      <div className="relative z-10 flex flex-1 flex-col px-10 pb-12 pt-14">
        <div>
          <p className="flex items-center gap-3 text-[11px] font-semibold uppercase tracking-[0.35em] text-white/55">
            <span className="h-px w-8 bg-white/40" />
            Steel Intelligence
          </p>
          <h1 className="mt-4 text-[30px] font-bold leading-tight text-white">
            让钢铁研发更聪明
            <span className="mt-1 block text-[20px] font-medium tracking-wide text-white/85">
              AI 驱动的材料研发助手
            </span>
          </h1>
          <div className="mt-6 flex flex-wrap gap-2">
            {["秒级预测", "智能寻优", "千篇文献", "产线数据"].map((item) => (
              <span
                key={item}
                className="rounded-full border border-white/20 bg-white/[0.08] px-3 py-1 text-xs tracking-wide text-white/85 backdrop-blur-sm"
              >
                {item}
              </span>
            ))}
          </div>
        </div>

        <div className="flex min-h-0 flex-1 items-center justify-center py-8">
          <img
            src="/Chat bot-amico.svg"
            alt="钢铁智能体机器人插画"
            className="w-full max-w-[350px] drop-shadow-[0_20px_40px_rgba(63,32,20,0.35)]"
            draggable={false}
          />
        </div>
      </div>
    </section>
  );
}
