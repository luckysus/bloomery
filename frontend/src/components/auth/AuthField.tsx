import type { ReactNode } from "react";

interface AuthFieldProps {
  icon: ReactNode;
  label?: string;
  required?: boolean;
  type?: string;
  value: string;
  placeholder: string;
  onChange: (value: string) => void;
  onEnter?: () => void;
  right?: ReactNode;
}

export function AuthField({
  icon,
  label,
  required = false,
  type = "text",
  value,
  placeholder,
  onChange,
  onEnter,
  right,
}: AuthFieldProps) {
  return (
    <label className="block">
      {label ? (
        <span className="mb-1.5 block text-[13px] font-medium tracking-normal text-[#6f6258]">
          {label}
          {required ? <span className="ml-1 text-[#cc785c]">*</span> : null}
        </span>
      ) : null}
      <span className="relative block">
        <span className="absolute left-3.5 top-1/2 -translate-y-1/2 text-[#8a7668]">{icon}</span>
        <input
          type={type}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") onEnter?.();
          }}
          placeholder={placeholder}
          className="h-12 w-full rounded-xl border border-[#e4d8cc] bg-[#fffaf3]/80 pl-11 pr-11 text-base text-[#2b2118] outline-none transition placeholder:text-[#b9a998] hover:border-[#d7c7b8] focus:border-[#cc785c]/55 focus:bg-[#fffdf8] focus:ring-4 focus:ring-[#cc785c]/12"
          style={{
            boxShadow: "inset 0 1px 2px rgba(126,66,47,0.08), 0 10px 24px rgba(91,69,53,0.05)",
          }}
        />
        {right ? <span className="absolute right-3.5 top-1/2 -translate-y-1/2">{right}</span> : null}
      </span>
    </label>
  );
}
