import { BookOpen, Info, Mic, Settings2, Sparkles } from 'lucide-react'

export const PANES = [
  { id: 'general', label: '通用', icon: Settings2 },
  { id: 'speech', label: '识别', icon: Mic },
  { id: 'polish', label: '润色', icon: Sparkles },
  { id: 'dictionary', label: '词典', icon: BookOpen },
  { id: 'about', label: '关于', icon: Info },
] as const

export type PaneId = (typeof PANES)[number]['id']
