'use client'

import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from 'react'
import { useTranslations } from 'next-intl'
import { MaximizeIcon, PictureInPicture2Icon, VideoIcon } from 'lucide-react'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'

import { apiIdSegment } from './api-path'
import type { Printer } from './dashboard-types'

export function CameraDialogControl({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const { openCamera } = useDashboardCamera()

  return (
    <Button
      className="min-h-8 w-full gap-2 rounded-md bg-muted/50 px-3 py-2 hover:bg-muted dark:hover:bg-muted"
      onClick={() => openCamera(printer)}
      type="button"
      variant="ghost"
    >
      <VideoIcon />
      {t('viewCamera')}
    </Button>
  )
}

type DashboardCameraContextValue = {
  openCamera: (printer: Printer) => void
}

const DashboardCameraContext = createContext<DashboardCameraContextValue | null>(null)

function useDashboardCamera() {
  const context = useContext(DashboardCameraContext)
  if (!context) {
    throw new Error('CameraDialogControl must be used within DashboardCameraProvider')
  }
  return context
}

export function DashboardCameraProvider({ children }: { children: ReactNode }) {
  const t = useTranslations('inventory')
  const frameRef = useRef<HTMLDivElement>(null)
  const videoRef = useRef<HTMLVideoElement>(null)
  const [printer, setPrinter] = useState<Printer | null>(null)
  const [open, setOpen] = useState(false)
  const pictureInPictureSupported =
    printer !== null &&
    document.pictureInPictureEnabled &&
    typeof HTMLVideoElement.prototype.requestPictureInPicture === 'function'

  useEffect(() => {
    if (!printer) return

    const video = videoRef.current
    const handleLeavePictureInPicture = () => {
      if (!open) setPrinter(null)
    }
    video?.addEventListener('leavepictureinpicture', handleLeavePictureInPicture)
    return () => video?.removeEventListener('leavepictureinpicture', handleLeavePictureInPicture)
  }, [open, printer])

  const handleOpenChange = (nextOpen: boolean) => {
    setOpen(nextOpen)
    if (!nextOpen && document.pictureInPictureElement !== videoRef.current) {
      setPrinter(null)
    }
  }

  const enterPictureInPicture = async () => {
    await videoRef.current?.requestPictureInPicture()
    setOpen(false)
  }

  const openCamera = (nextPrinter: Printer) => {
    setPrinter(nextPrinter)
    setOpen(true)
  }

  return (
    <DashboardCameraContext.Provider value={{ openCamera }}>
      {children}
      <Dialog open={open} onOpenChange={handleOpenChange}>
        {printer ? (
          <DialogContent className="sm:max-w-3xl" closeLabel={t('closeCamera')} keepMounted>
            <DialogHeader>
              <DialogTitle>{t('cameraTitle')}</DialogTitle>
            </DialogHeader>
            <div ref={frameRef} className="relative overflow-hidden rounded-md bg-black">
              <video
                ref={videoRef}
                aria-label={t('cameraTitle')}
                autoPlay
                className="aspect-video w-full bg-black object-contain"
                muted
                playsInline
                src={`/api/tenants/${apiIdSegment(printer.tenant_id, 'tenant_id')}/printers/${apiIdSegment(printer.id, 'printer_id')}/camera.mp4`}
              />
              <div className="absolute right-3 top-3 flex gap-2">
                {pictureInPictureSupported ? (
                  <Button
                    aria-label={t('cameraPictureInPicture')}
                    className="rounded-md bg-black/70 text-white hover:bg-black/90 hover:text-white dark:hover:bg-black/90"
                    onClick={() => void enterPictureInPicture()}
                    size="icon"
                    title={t('cameraPictureInPicture')}
                    type="button"
                    variant="ghost"
                  >
                    <PictureInPicture2Icon className="size-4" />
                  </Button>
                ) : null}
                <Button
                  aria-label={t('cameraFullscreen')}
                  className="rounded-md bg-black/70 text-white hover:bg-black/90 hover:text-white dark:hover:bg-black/90"
                  onClick={() => void frameRef.current?.requestFullscreen()}
                  size="icon"
                  title={t('cameraFullscreen')}
                  type="button"
                  variant="ghost"
                >
                  <MaximizeIcon className="size-4" />
                </Button>
              </div>
            </div>
          </DialogContent>
        ) : null}
      </Dialog>
    </DashboardCameraContext.Provider>
  )
}
