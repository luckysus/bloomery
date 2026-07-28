import { X } from "lucide-react";
import { useEffect } from "react";

export type LegalDocKind = "terms" | "privacy";

const EFFECTIVE_DATE = "2026年7月24日";
const SUBJECT_NAME = "钢铁智能体";
const CONTACT_EMAIL = "1853097312@qq.com";

interface LegalSection {
  heading: string;
  paragraphs?: string[];
  bullets?: string[];
}

const TERMS_SECTIONS: LegalSection[] = [
  {
    heading: "1. 服务说明",
    paragraphs: [
      `${SUBJECT_NAME}（以下简称"本平台"）是一个面向钢铁领域的智能问答与知识管理平台，提供文献上传解析、知识库检索、AI 智能问答、语音输入等功能。`,
    ],
  },
  {
    heading: "2. 账号注册与安全",
    bullets: [
      "您需提供真实信息注册账号，并对账号下的一切活动负责。",
      "请妥善保管账号密码，因您自身原因导致的账号泄露，本平台不承担责任。",
    ],
  },
  {
    heading: "3. 用户行为规范",
    paragraphs: ["您承诺不利用本平台从事以下行为："],
    bullets: [
      "上传违法、侵权、涉密或含病毒的内容；",
      "干扰、破坏平台正常运行或进行未授权访问；",
      "侵犯他人知识产权或个人信息。",
    ],
  },
  {
    heading: "4. AI 生成内容免责",
    bullets: [
      "本平台问答结果由第三方大模型生成，可能存在错误、遗漏或不准确之处，仅供参考，不构成专业（技术/工程/法律等）意见。",
      "您应对依据 AI 结果作出的任何决策自行承担风险。",
    ],
  },
  {
    heading: "5. 知识产权",
    bullets: [
      "您上传的文献/资料的知识产权归您或原权利人所有；您需保证有权上传。",
      "本平台的软件、界面、设计等知识产权归本平台所有。",
    ],
  },
  {
    heading: "6. 责任限制",
    paragraphs: [
      "在法律允许的最大范围内，本平台对因使用/无法使用服务造成的间接损失不承担责任。",
    ],
  },
  {
    heading: "7. 服务变更与终止",
    paragraphs: [
      "本平台有权对违规账号采取警告、限制或封禁措施，并保留调整或终止服务的权利。",
    ],
  },
  {
    heading: "8. 法律适用",
    paragraphs: [
      "本条款适用中华人民共和国法律，争议由本平台所在地法院管辖。",
    ],
  },
  {
    heading: "9. 联系方式",
    paragraphs: [`如有疑问，请联系：${CONTACT_EMAIL}`],
  },
];

const PRIVACY_SECTIONS: LegalSection[] = [
  {
    heading: "1. 我们收集的信息",
    bullets: [
      "账号信息：用户名、密码（加密存储）；",
      "内容信息：您上传的文献、图片、文档；",
      "语音信息：您使用语音输入时的音频（用于识别为文字）；",
      "交互信息：您与 AI 的对话内容、检索记录；",
      "日志信息：IP、访问时间、设备/浏览器等技术日志。",
    ],
  },
  {
    heading: "2. 我们如何使用信息",
    paragraphs: [
      "用于提供并改进核心服务：文献解析、知识检索、AI 问答、语音转文字，以及保障账号安全和服务稳定。",
    ],
  },
  {
    heading: "3. 第三方共享（重要）",
    paragraphs: ["为实现功能，部分数据会传输给以下第三方处理者，仅用于对应功能："],
    bullets: [
      "大模型服务商（如 DeepSeek 等）：处理您的对话内容以生成回答；",
      "讯飞开放平台：处理您的语音音频以转写为文字；",
      "文档解析服务：处理您上传的文献以提取内容。",
    ],
  },
  {
    heading: "4. 信息存储与安全",
    bullets: [
      "数据存储于中国境内服务器；密码采用加密方式保存。",
      "我们采取合理的技术与管理措施保护您的信息安全。",
    ],
  },
  {
    heading: "5. 您的权利",
    paragraphs: [
      "您有权访问、更正、删除您的个人信息，或注销账号。注销后我们将依法删除或匿名化您的数据。",
    ],
  },
  {
    heading: "6. Cookie",
    paragraphs: [
      "我们使用必要的 Cookie/本地存储维持登录状态与偏好设置。",
    ],
  },
  {
    heading: "7. 未成年人",
    paragraphs: [
      "本平台主要面向成年专业用户，若您未满 18 岁，请在监护人指导下使用。",
    ],
  },
  {
    heading: "8. 政策更新",
    paragraphs: [
      "本政策如有更新，我们将在页面公示；重大变更会另行提示。",
    ],
  },
  {
    heading: "9. 联系我们",
    paragraphs: [`如需行使权利或有隐私相关问题，请联系：${CONTACT_EMAIL}`],
  },
];

const DOC_META: Record<LegalDocKind, { title: string; sections: LegalSection[] }> = {
  terms: { title: `${SUBJECT_NAME} 服务条款`, sections: TERMS_SECTIONS },
  privacy: { title: `${SUBJECT_NAME} 隐私政策`, sections: PRIVACY_SECTIONS },
};

interface LegalDocsModalProps {
  doc: LegalDocKind | null;
  onClose: () => void;
}

export default function LegalDocsModal({ doc, onClose }: LegalDocsModalProps) {
  useEffect(() => {
    if (!doc) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [doc, onClose]);

  if (!doc) return null;
  const meta = DOC_META[doc];

  return (
    <div
      className="fixed inset-0 z-[110] flex items-center justify-center bg-[#2b2118]/50 px-4 py-10 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="flex max-h-full w-full max-w-[720px] flex-col overflow-hidden rounded-[28px] border border-[#e6d8ca] bg-[#f8f1e8] shadow-[0_30px_80px_rgba(72,52,38,0.30),inset_0_1px_0_rgba(255,255,255,0.85)]"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-4 px-8 pb-5 pt-7">
          <div>
            <h3 className="text-[22px] font-bold tracking-tight text-[#2b2118]">{meta.title}</h3>
            <p className="mt-1.5 text-sm text-[#8a7c6d]">生效日期：{EFFECTIVE_DATE}</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="mt-1 flex h-9 w-9 shrink-0 items-center justify-center rounded-full text-[#6f6258] transition hover:text-[#cc785c]"
            style={{ boxShadow: "3px 3px 6px #d8c9ba, -3px -3px 6px #fffaf2" }}
            title="关闭"
          >
            <X size={18} />
          </button>
        </div>

        <div className="mx-8 border-t border-[#eadfd2]" />

        <div className="overflow-y-auto px-8 py-6 text-[15px] leading-relaxed text-[#4a3f34]">
          {meta.sections.map((section) => (
            <section key={section.heading} className="mb-6 last:mb-0">
              <h4 className="mb-2 text-base font-semibold text-[#2b2118]">{section.heading}</h4>
              {section.paragraphs?.map((text, index) => (
                <p key={index} className="mb-2 last:mb-0">
                  {text}
                </p>
              ))}
              {section.bullets && (
                <ul className="ml-5 list-disc space-y-1.5 marker:text-[#cbb9a4]">
                  {section.bullets.map((item, index) => (
                    <li key={index}>{item}</li>
                  ))}
                </ul>
              )}
            </section>
          ))}
        </div>
      </div>
    </div>
  );
}
