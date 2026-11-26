import { Paperclip, Download, X } from 'lucide-react';
import { useBackend } from '@/lib/workspace-backend';
import type { ApiMessage } from '@/lib/api';

export function BackendFilePicker({files,onChange,disabled}: {files:File[];onChange:(files:File[])=>void;disabled:boolean}) {
  const backend = useBackend();
  return <div className="flex flex-wrap items-center gap-2"><label className="workspace-icon-button relative rounded-md text-[#c8b2f1] focus-within:outline-2 focus-within:outline-[#c8b2f1]" title="Tối đa 5 tệp, 25 MB mỗi tệp"><Paperclip size={16}/><input type="file" multiple aria-label="Đính kèm tệp" disabled={disabled} className="absolute inset-0 opacity-0" onChange={event => {const next=[...files,...Array.from(event.target.files||[])];event.target.value='';if(next.length>5||next.some(file=>file.size===0||file.size>25*1024*1024)){backend?.report(new Error('Chọn tối đa 5 tệp, mỗi tệp từ 1 byte đến 25 MB.'));return;}onChange(next);}}/></label>{files.map((file,index)=><span key={index} className="flex max-w-56 items-center gap-2 rounded-lg border border-white/15 px-2 py-1 text-xs text-[#c8b2f1]"><span className="truncate">{file.name}</span><button type="button" disabled={disabled} aria-label={`Bỏ tệp ${file.name}`} onClick={()=>onChange(files.filter((_,i)=>i!==index))}><X size={13}/></button></span>)}</div>;
}
export function BackendAttachments({attachments}: {attachments:ApiMessage['attachments']}) {
  const backend = useBackend();
  return <div className="space-y-2">{attachments.map(file=><button key={file.attachment_id} type="button" className="mt-3 flex w-full items-center gap-3 rounded-xl border border-white/15 bg-[#15121d] p-3 text-left text-sm text-[#f4effa]" onClick={async()=>{if(!backend)return;try{const value=await backend.api.request<{url:string}>(`/attachments/${file.attachment_id}/url`);const blob=await backend.api.download(value.url);const url=URL.createObjectURL(blob);const anchor=document.createElement('a');anchor.href=url;anchor.download=file.file_name;anchor.click();setTimeout(()=>URL.revokeObjectURL(url),60000);}catch(error){backend.report(error);}}}><Download size={16}/><span className="break-all">{file.file_name}<small className="mt-1 block text-[#a2d9d3]">{(file.file_size_bytes/1024).toLocaleString('vi-VN',{maximumFractionDigits:1})} KB</small></span></button>)}</div>;
}
