import { Component, type ReactNode, type ErrorInfo } from 'react'

interface Props {
  children: ReactNode
}

interface State {
  hasError: boolean
  error: Error | null
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props)
    this.state = { hasError: false, error: null }
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('ErrorBoundary caught:', error, info.componentStack)
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="p-8 flex flex-col items-center justify-center h-screen font-sans text-text-primary">
          <h2 className="text-[17px] font-medium mb-2">界面出现了一点问题</h2>
          <p className="text-text-secondary mb-4 text-[13px]">
            {this.state.error?.message || '发生了未预期的错误'}
          </p>
          <button
            onClick={() => window.location.reload()}
            className="px-4 py-2 rounded-[10px] border border-border bg-bg-secondary cursor-pointer text-[13px] hover:bg-bg-elevated transition-colors"
          >
            重新加载
          </button>
        </div>
      )
    }

    return this.props.children
  }
}
