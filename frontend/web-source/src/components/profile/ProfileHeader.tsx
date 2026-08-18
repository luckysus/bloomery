import { X } from "lucide-react";

interface ProfileHeaderProps {
  onClose: () => void;
}

export function ProfileHeader({ onClose }: ProfileHeaderProps) {
  return (
    <div className="flex items-center justify-between border-b border-slate-200 px-6 py-4">
      <h3 className="text-base font-bold text-slate-900">系统设置</h3>
      <button onClick={onClose} className="flex h-9 w-9 items-center justify-center rounded-lg text-slate-400 hover:bg-slate-100 hover:text-slate-700">
        <X size={20} />
      </button>
    </div>
  );
}
